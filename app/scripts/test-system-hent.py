#!/usr/bin/python3
"""Prøver for system-hent.py uten nett, root eller systemfiler."""
import contextlib
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch

DATA = Path(__file__).resolve().parents[1] / 'data/hjelper'
NAME = 'agentic-rigg-1.5.2-build27-ubuntu26.04-amd64.tar.gz'
UUID = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
LIB = '/usr/local/libexec/agentic-rigg-rgb-v2/'
IDENTITY = json.dumps({'name': 'Agentic HW Monitor', 'version': '1.5.2', 'channel': 'stable', 'build': 27}).encode()
FILES = {'version.json': IDENTITY, 'bin/agentic-rigg-helper': b'hjelper', 'data/hjelper/system-install': b'print("installert")\n',
         'data/hjelper/aktiver.sh': b'#!/bin/sh\n', 'README.md': b'les meg', 'data/icons/x.svg': b'<svg/>'}


def archive(installer=b'print("installert")\n', extra=(), files=None, root='agentic-rigg-1.5.2'):
    files = dict(FILES if files is None else files)
    if installer is not None:
        files['data/hjelper/system-install'] = installer
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode='w:gz') as tar:
        for name, data in (*((f'{root}/{key}', data) for key, data in files.items()), *extra):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def pins(files=None):
    """SYSTEMFILER.sha256 slik pakkeskriptet lager den."""
    files = FILES if files is None else files
    return ''.join(f'{hashlib.sha256(data).hexdigest()}  {key}\n' for key, data in sorted(files.items())
                   if key.startswith(('bin/', 'data/hjelper/')) or key == 'version.json')


class Response(io.BytesIO):
    def __init__(self, data, url):
        super().__init__(data)
        self.url = url

    def geturl(self):
        return self.url

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


class HentTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory(dir='/tmp')
        self.addCleanup(tmp.cleanup)
        self.staging = Path(tmp.name)
        self.mod = runpy.run_path(str(DATA / 'system-hent.py'), run_name='test')['main'].__globals__
        self.mod['STAGING'] = str(self.staging)
        self.data = archive()
        self.digest = hashlib.sha256(self.data).hexdigest()
        self.assets = [{'name': NAME, 'digest': 'sha256:' + self.digest}]
        self.urls = []
        self.runs = []
        self.deactivate_code = 0
        self.inspect = b'{}'
        self.fail_at = None
        patcher = patch('os.geteuid', return_value=0)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.mod['open_url'] = self.open_url
        self.mod['subprocess'] = type('S', (), {'run': self.fake_run, 'DEVNULL': -3, 'PIPE': -1})
        # Aldri vertens /etc/agentic-rigg: tillatelsene ligger i testmappa.
        self.etc = Path(tmp.name + '-etc')
        self.etc.mkdir(mode=0o755)
        self.addCleanup(shutil.rmtree, self.etc)
        self.mod['CONFIG'] = {'nvml': str(self.etc / 'helper.json'), 'rgb': str(self.etc / 'rgb.json')}
        self.mod['OWNER'] = os.getuid()
        self.marker = self.etc / 'revision'
        self.mod['MARK'] = str(self.marker)
        self.journal = self.etc / 'journal.json'
        self.mod['RGB_JOURNAL'] = str(self.journal)
        # Aldri vertens /run/agentic-rigg-system.lock.
        self.lock = self.etc / 'system.lock'
        self.mod['LOCK'] = str(self.lock)
        self.answer = None
        self.asked = []
        self.mod['confirmation'] = self.confirm
        uid = patch.dict(os.environ, {'PKEXEC_UID': '1000'})
        uid.start()
        self.addCleanup(uid.stop)

    def confirm(self, timeout=None):
        # Spørsmålet kommer først etter gjenopprettingen og avlesningen.
        self.asked.append([Path(r[0]).name + (' ' + r[-1] if r[-1] == '--inspect' else '') for r in self.runs])
        return self.answer

    def lock_held(self):
        """Holder noen den felles låsen (en annen åpen fil kan ikke ta den)?"""
        if not self.lock.exists():
            return False
        with self.lock.open('rb') as probe:
            try:
                fcntl.flock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return True
            fcntl.flock(probe, fcntl.LOCK_UN)
            return False

    def main(self, *args):
        """system-hent med appens argumenter: VERSJON BYGG FESTET [domene pci uuid]."""
        return self.mod['main'](['1.5.2', '27', '-', *args])

    def open_url(self, request, hosts):
        url = request.full_url
        self.hosts = hosts
        self.urls.append(url)
        self.assertEqual(request.get_header('User-agent'), 'agentic-rigg-system-install')
        if url.startswith('https://api.github.com/'):
            return Response(json.dumps({'assets': self.assets}).encode(), url)
        self.assertEqual(url, f'https://github.com/bejulian1/agentic-rigg/releases/download/v1.5.2/{NAME}')
        self.assertEqual(hosts, ('github.com', '.githubusercontent.com'))
        return Response(self.data, 'https://release-assets.githubusercontent.com/x')

    def fake_run(self, argv, **kwargs):
        self.runs.append(argv)
        old_scripts = argv[0].endswith(('rgb-gjenopprett.sh', 'rgb-avklar.sh', 'rgb-deaktiver.sh'))
        if argv[0].startswith('/usr/local/libexec/') and old_scripts:
            # De gamle, installerte RGB-skriptene tar låsen selv: den er ledig.
            self.assertEqual(kwargs['env'], {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'PKEXEC_UID': '1000'})
            self.assertFalse(kwargs.get('pass_fds'))
            self.assertFalse(self.lock_held())
        else:
            # Installasjon og aktivering skjer under én holdt lås, som de
            # nyinstallerte skriptene arver.
            fds = kwargs['pass_fds']
            self.assertEqual(len(fds), 1)
            self.assertEqual(kwargs['env']['AGENTIC_RIGG_SYSTEM_LOCK_FD'], str(fds[0]))
            self.assertEqual(os.fstat(fds[0]).st_ino, self.lock.stat().st_ino)
            self.assertTrue(self.lock_held())
            fcntl.flock(fds[0], fcntl.LOCK_EX | fcntl.LOCK_NB)  # Samme åpne fil: allerede holdt.
            env = dict(kwargs['env'])
            del env['AGENTIC_RIGG_SYSTEM_LOCK_FD']
            kwargs = {**kwargs, 'env': env}
        if argv[0].startswith('/usr/local/libexec/'):
            self.assertEqual(kwargs['env'], {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'PKEXEC_UID': '1000'})
            if argv[0].endswith(('rgb-gjenopprett.sh', 'rgb-avklar.sh')):
                # Bare etter kontrollert arkiv: installasjonsprogrammet hentes
                # ut først når summen stemmer.
                [temp] = list(self.staging.iterdir())
                self.assertTrue((temp / 'installer').is_file())
                step = 'inspect' if argv[-1] == '--inspect' else Path(argv[0]).stem
                if step == self.fail_at:
                    return type('R', (), {'returncode': 1, 'stdout': b''})()
                if argv[-1] == '--inspect':
                    self.assertEqual(kwargs['stdout'], -1)
                    if self.inspect == b'{}':
                        self.journal.unlink()  # Tom rapport: ingen uavklart journal.
                    return type('R', (), {'returncode': 0, 'stdout': self.inspect})()
                if argv[0].endswith('rgb-avklar.sh'):
                    self.journal.unlink()
                return type('R', (), {'returncode': 0})()
            if argv[0].endswith('rgb-deaktiver.sh'):
                # Etter kontrollen av arkivet, før installasjonen.
                self.assertEqual(len(list(self.staging.iterdir())), 1)
                if not self.deactivate_code:
                    # Som det ekte skriptet: tillatelsen fjernes, journalen må være avklart.
                    self.assertFalse(self.journal.exists())
                    Path(self.mod['CONFIG']['rgb']).unlink()
                return type('R', (), {'returncode': self.deactivate_code})()
            self.assertEqual(list(self.staging.iterdir()), [])
            return type('R', (), {'returncode': 0})()
        self.assertEqual(Path(argv[-3]).read_bytes(), b'print("installert")\n')
        self.assertEqual(hashlib.sha256(Path(argv[-2]).read_bytes()).hexdigest(), argv[-1])
        self.assertEqual(oct(os.stat(Path(argv[-3]).parent).st_mode & 0o777), '0o700')
        self.assertEqual(kwargs['env'], {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
        return type('R', (), {'returncode': 0})()

    def test_henter_kontrollerer_og_installerer(self):
        self.assertEqual(self.main(), 0)
        self.assertEqual(self.urls[0], 'https://api.github.com/repos/bejulian1/agentic-rigg/releases/tags/v1.5.2')
        self.assertEqual(self.runs[0][:3], ['/usr/bin/python3', '-I', '-S'])
        self.assertEqual(self.runs[0][-1], self.digest)
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_installerer_og_aktiverer_med_ett_passord(self):
        pci, uuid = '0000:1C:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        with patch.dict(os.environ, {'PKEXEC_UID': '1000'}):
            self.assertEqual(self.main('nvml', pci, uuid), 0)
            self.assertEqual(self.runs[-1], ['/usr/local/libexec/agentic-rigg/aktiver.sh', '--pci', pci.lower(), '--uuid', uuid])
            self.assertEqual(self.main('rgb', pci, uuid), 0)
            self.assertEqual(self.runs[-1], ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-aktiver.sh', '--pci', pci.lower(),
                                             '--uuid', uuid, '--static-confirmed'])
        self.assertEqual(len(self.runs), 4)

    def permission(self, domain, uid=1000, pci='0000:1c:00.0', uuid='GPU-12345678-abcd-ef01-2345-6789abcdef01', mode=0o644):
        card = {'pci': pci, 'uuid': uuid}
        data = {'version': 1, 'uid': uid, 'enabled': True, 'binding': card, 'build': 28} if domain == 'rgb' else \
            {'versjon': 2, 'uid': uid, 'enabled': True, **card, 'bygg': 28}
        path = Path(self.mod['CONFIG'][domain])
        path.write_text(json.dumps(data))
        path.chmod(mode)

    def test_oppdatering_beholder_gpu_styring_og_slaar_av_gammel_rgb(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        other = 'GPU-87654321-abcd-ef01-2345-6789abcdef01'
        self.permission('nvml', pci='0000:01:00.0', uuid=other)
        self.permission('rgb', mode=0o600)
        self.assertEqual(self.main('rgb', '0000:1c:00.0', uuid), 0)
        self.assertEqual(self.runs[0], ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-deaktiver.sh', '--pci', '0000:1c:00.0', '--uuid', uuid])
        self.assertEqual(self.runs[1][:3], ['/usr/bin/python3', '-I', '-S'])
        self.assertEqual(self.runs[2], ['/usr/local/libexec/agentic-rigg/aktiver.sh', '--pci', '0000:01:00.0', '--uuid', other])
        self.assertEqual(self.runs[3][0], '/usr/local/libexec/agentic-rigg-rgb-v2/rgb-aktiver.sh')
        # Ber brukeren selv om GPU-styring, aktiveres den bare én gang.
        self.runs.clear()
        self.permission('rgb', mode=0o600)
        self.assertEqual(self.main('nvml', '0000:1c:00.0', uuid), 0)
        self.assertEqual([r[0] for r in self.runs], ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-deaktiver.sh', '/usr/bin/python3',
                                                     '/usr/local/libexec/agentic-rigg/aktiver.sh'])
        self.assertEqual(self.runs[-1][2], '0000:1c:00.0')

    def test_uavklart_rgb_stopper_for_installasjonen(self):
        self.permission('rgb', mode=0o600)
        self.deactivate_code = 1
        self.assertEqual(self.main(), 1)
        self.assertEqual(len(self.runs), 1)
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_nedgradering_stopper_for_nedlasting_og_rgb_endringer(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        self.marker.write_text('2:28\n')
        self.marker.chmod(0o644)
        with self.assertRaisesRegex(SystemExit, 'nedgradere.*28.*27'):
            self.main('rgb', '0000:1c:00.0', uuid)
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])
        self.assertTrue(self.journal.exists())

    def test_nyere_revisjon_under_nedlasting_stopper_rgb_endringer(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        self.marker.write_text('2:27\n')
        self.marker.chmod(0o644)

        def upgraded_during_download(request, hosts):
            response = self.open_url(request, hosts)
            if request.full_url.startswith('https://github.com/'):
                self.marker.write_text('2:28\n')
            return response

        self.mod['open_url'] = upgraded_during_download
        with self.assertRaisesRegex(SystemExit, 'nedgradere.*28.*27'):
            self.main('rgb', '0000:1c:00.0', uuid)
        self.assertEqual(len(self.urls), 2)
        self.assertEqual(self.runs, [])
        self.assertTrue(self.journal.exists())
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_uavklart_okt_avsluttes_med_brukerens_bekreftelse(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        expected = {'mode': 'static', 'color': [1, 2, 3], 'brightness': 80}
        self.inspect = json.dumps({'expected': expected, 'recoveryId': 'a' * 32}).encode()
        self.answer = 'a' * 32
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(self.main('rgb', '0000:1c:00.0', uuid), 0)
        ids = ['--pci', '0000:1c:00.0', '--uuid', uuid]
        lib = '/usr/local/libexec/agentic-rigg-rgb-v2/'
        self.assertEqual(self.runs[:4], [[lib + 'rgb-gjenopprett.sh', *ids, '--restore-original'],
                                         [lib + 'rgb-avklar.sh', *ids, '--inspect'],
                                         [lib + 'rgb-avklar.sh', *ids, '--static-confirmed', '--recovery-id', 'a' * 32],
                                         [lib + 'rgb-deaktiver.sh', *ids]])
        self.assertEqual(self.runs[-1][0], lib + 'rgb-aktiver.sh')
        # Brukeren ble spurt etter gjenopprettingen og før avklaringen, og
        # appen fikk det gjenopprettede målet for nettopp dette kortet.
        self.assertEqual(self.asked, [['rgb-gjenopprett.sh', 'rgb-avklar.sh --inspect']])
        [line] = [l for l in out.getvalue().splitlines() if l.startswith('AGENTIC-RIGG-RGB-AVKLAR ')]
        self.assertEqual(json.loads(line.split(' ', 1)[1]),
                         {'pci': '0000:1c:00.0', 'uuid': uuid, 'expected': expected, 'recoveryId': 'a' * 32})

    def test_avklaring_registreres_aldri_uten_bekreftelse_av_gjenopprettet_lys(self):
        self.permission('rgb', mode=0o600)
        self.inspect = json.dumps({'expected': {'mode': 'static', 'color': [0, 0, 255], 'brightness': 100},
                                   'recoveryId': 'c' * 32}).encode()
        # Ingen svar (lukket stdin eller tidsavbrudd), en annen identitet, eller tom linje.
        for answer in (None, 'd' * 32, '', 'c' * 31):
            self.runs.clear()
            self.journal.write_text('{}')
            self.answer = answer
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(self.main('rgb', '0000:1c:00.0', UUID), 1)
            self.assertEqual([Path(r[0]).name for r in self.runs], ['rgb-gjenopprett.sh', 'rgb-avklar.sh'])
            self.assertEqual(self.runs[-1][-1], '--inspect')
            self.assertTrue(self.journal.exists())
            self.assertEqual(list(self.staging.iterdir()), [])
            self.assertFalse(self.lock_held())

    def test_uavklart_okt_feil_trinn_stopper_resten(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.inspect = json.dumps({'expected': {'mode': 'off'}, 'recoveryId': 'b' * 32}).encode()
        lib = '/usr/local/libexec/agentic-rigg-rgb-v2/'
        self.answer = 'b' * 32
        for step, ran in (('rgb-gjenopprett', 1), ('inspect', 2), ('rgb-avklar', 3)):
            self.runs.clear()
            self.journal.write_text('{}')
            self.fail_at = step
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(self.main('rgb', '0000:1c:00.0', uuid), 1)
            self.assertEqual(len(self.runs), ran, step)
            self.assertTrue(all(r[0].startswith(lib) and 'deaktiver' not in r[0] for r in self.runs))
            self.assertEqual(list(self.staging.iterdir()), [])
        # Tom rapport: ingen avklaring, men deaktivering og installasjon.
        self.fail_at, self.inspect = None, b'{}'
        self.runs.clear()
        self.assertEqual(self.main('rgb', '0000:1c:00.0', uuid), 0)
        self.assertEqual([Path(r[0]).name for r in self.runs[:4]],
                         ['rgb-gjenopprett.sh', 'rgb-avklar.sh', 'rgb-deaktiver.sh', 'python3'])

    def test_uavklart_okt_og_feil_sum_rorer_ikke_kortet(self):
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        self.assets[0]['digest'] = 'sha256:' + '0' * 64
        with self.assertRaisesRegex(SystemExit, 'Kontrollsummen stemmer ikke'):
            self.main('rgb', '0000:1c:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01')
        self.assertEqual(self.runs, [])

    def test_uavklart_okt_uten_bekreftelse_stopper_for_nettet(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        other = 'GPU-87654321-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        # GPU-styring, ingen aktivering, eller et annet kort enn RGB-tillatelsen.
        for extra in (['nvml', '0000:1c:00.0', uuid], [], ['rgb', '0000:1c:00.0', other]):
            with self.assertRaisesRegex(SystemExit, 'kortet 0000:1c:00.0 .*åpne RGB-lys'):
                self.main(*extra)
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_ugyldig_avklaringsmal_beholder_journalen(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        for report in (b'{"recoveryId": "../x"}', b'[]', b'ikke json', json.dumps({'recoveryId': 'a' * 32}).encode(),
                       json.dumps({'recoveryId': 'a' * 32, 'expected': 'static'}).encode()):
            self.runs.clear()
            self.inspect = report
            with self.assertRaisesRegex(SystemExit, 'ugyldig avklaringsmål'):
                self.main('rgb', '0000:1c:00.0', uuid)
            self.assertEqual(len(self.runs), 2)
        self.assertTrue(self.journal.exists())
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_fremmed_eller_utrygg_tillatelse_stopper_for_nettet(self):
        for domain, kwargs in (('nvml', {'uid': 1001}), ('rgb', {'uid': 1001}), ('nvml', {'mode': 0o666}),
                               ('rgb', {'pci': '../x'}), ('nvml', {'uuid': 'GPU-x'})):
            for path in self.etc.iterdir():
                path.unlink()
            self.permission(domain, **kwargs)
            with self.assertRaises(SystemExit):
                self.main()
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_eldre_tillatelse_for_samme_bruker_faar_egen_melding(self):
        path = self.etc / 'helper.json'
        path.write_text(json.dumps({'versjon': 1, 'uid': 1000}))
        path.chmod(0o600)
        with self.assertRaisesRegex(SystemExit, 'eldre oppsett'):
            self.main()
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_aktivering_bare_etter_vellykket_installasjon(self):
        def failing(argv, **kwargs):
            self.runs.append(argv)
            return type('R', (), {'returncode': 3})()
        self.mod['subprocess'] = type('S', (), {'run': failing, 'DEVNULL': -3})
        with patch.dict(os.environ, {'PKEXEC_UID': '1000'}):
            self.assertEqual(self.main('nvml', '0000:1c:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01'), 3)
        self.assertEqual(len(self.runs), 1)

    def test_ugyldig_aktivering_stopper_for_nettet(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        for extra, env in ((['gpu', '0000:1c:00.0', uuid], '1000'), (['nvml', '1c:00.0', uuid], '1000'),
                           (['nvml', '0000:1c:00.0', 'GPU-x'], '1000'), (['nvml', '0000:1c:00.0', uuid], ''),
                           (['nvml', '0000:1c:00.0', uuid], '0'), (['rgb', '0000:1c:00.0', uuid, '--x'], '1000')):
            with patch.dict(os.environ, {'PKEXEC_UID': env}), self.assertRaises(SystemExit):
                self.main(*extra)
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_feil_sum_kjorer_ingenting(self):
        self.assets[0]['digest'] = 'sha256:' + '0' * 64
        with self.assertRaisesRegex(SystemExit, 'Kontrollsummen stemmer ikke'):
            self.main()
        self.assertEqual(self.runs, [])
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_mangler_arkiv_eller_sum(self):
        for assets in ([], [{'name': NAME}], [{'name': NAME, 'digest': 'sha512:' + 'a' * 64}], self.assets * 2):
            self.assets = assets
            with self.assertRaises(SystemExit):
                self.main()
        self.assertEqual(self.runs, [])

    def test_videresending_bare_til_github_over_https(self):
        ok = self.mod['trusted_host']
        hosts = ('github.com', '.githubusercontent.com')
        self.assertTrue(ok('https://release-assets.githubusercontent.com/a', hosts))
        self.assertTrue(ok('https://github.com/a', hosts))
        for url in ('http://github.com/a', 'https://evilgithub.com/a', 'https://githubusercontent.com.evil.io/a',
                    'https://github.com.evil.io/a', 'ftp://github.com/a'):
            self.assertFalse(ok(url, hosts), url)
        redirects = self.mod['Redirects'](())
        with self.assertRaisesRegex(SystemExit, 'ukjent adresse'):
            redirects.redirect_request(None, None, 302, '', {}, 'https://api.github.com/annet')

    def test_manglende_utgivelse_og_nettbrudd(self):
        import urllib.error, http.client
        def missing(request, hosts):
            error = urllib.error.HTTPError(request.full_url, 404, 'Not Found', {}, None)
            self.addCleanup(error.close)
            raise error
        self.mod['open_url'] = missing
        with self.assertRaisesRegex(SystemExit, 'finnes ikke på GitHub'):
            self.main()
        def broken(request, hosts):
            raise http.client.IncompleteRead(b'')
        self.mod['open_url'] = broken
        with self.assertRaisesRegex(SystemExit, 'Fikk ikke kontakt med GitHub'):
            self.main()

    def test_ugyldige_argumenter_og_ikke_root(self):
        for args in ([], ['1.5.2', '27'], ['1.5', '27', '-'], ['１.５.２', '27', '-'], ['1.5.2', '0', '-'], ['1.5.2;x', '27', '-'],
                     ['1.5.2', '27', '-', 'x'], ['../1.5.2', '27', '-']):
            with self.assertRaisesRegex(SystemExit, 'VERSJON BYGG'):
                self.mod['main'](args)
        with patch('os.geteuid', return_value=1000), self.assertRaisesRegex(SystemExit, 'administrator'):
            self.main()
        self.assertEqual(self.urls, [])

    def test_for_stort_arkiv_og_tvetydig_installasjon(self):
        self.mod['MAX_ARCHIVE'] = 10
        with self.assertRaisesRegex(SystemExit, 'større enn forventet'):
            self.main()
        self.mod['MAX_ARCHIVE'] = 64 << 20
        self.data = archive(extra=(('annen/data/hjelper/system-install', b'x'),))
        self.assets[0]['digest'] = 'sha256:' + hashlib.sha256(self.data).hexdigest()
        with self.assertRaisesRegex(SystemExit, 'uventet struktur'):
            self.main()
        self.assertEqual(self.runs, [])
        self.assertEqual(list(self.staging.iterdir()), [])


    # --- Én felles systemlås og nye kontroller etter nedlastingen (#18) ---

    def test_tilbakekalt_gpu_tilgang_under_nedlasting_bestaar(self):
        self.permission('nvml', pci='0000:01:00.0')
        config = Path(self.mod['CONFIG']['nvml'])

        def revoked_during_download(request, hosts):
            response = self.open_url(request, hosts)
            if request.full_url.startswith('https://github.com/'):
                config.unlink()  # En annen administratorhandling slo av GPU-styringen.
            return response

        self.mod['open_url'] = revoked_during_download
        for extra in ([], ['rgb', '0000:1c:00.0', UUID]):
            self.permission('nvml', pci='0000:01:00.0')
            with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg'):
                self.main(*extra)
            self.assertEqual(self.runs, [])
            self.assertFalse(config.exists())
            self.assertEqual(list(self.staging.iterdir()), [])
            self.assertFalse(self.lock_held())

    def test_endret_tillatelse_under_nedlasting_stopper_for_rgb_endringer(self):
        self.permission('rgb', mode=0o600)
        self.permission('nvml', pci='0000:01:00.0')

        def rebound_during_download(request, hosts):
            response = self.open_url(request, hosts)
            if request.full_url.startswith('https://github.com/'):
                self.permission('nvml', pci='0000:02:00.0')
            return response

        self.mod['open_url'] = rebound_during_download
        with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg'):
            self.main('rgb', '0000:1c:00.0', UUID)
        self.assertEqual(self.runs, [])

    def test_endring_mens_rgb_slaas_av_stopper_for_installasjonen(self):
        self.permission('rgb', mode=0o600)
        self.permission('nvml', pci='0000:01:00.0')
        config = Path(self.mod['CONFIG']['nvml'])
        run = self.fake_run

        def revoke_after_rgb(argv, **kwargs):
            result = run(argv, **kwargs)
            if argv[0].endswith('rgb-deaktiver.sh'):
                config.unlink()  # GPU-tilgangen trekkes tilbake før låsen er tatt.
            return result

        self.mod['subprocess'] = type('S', (), {'run': revoke_after_rgb, 'DEVNULL': -3, 'PIPE': -1})
        with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg.*RGB-tilgangen er slått av'):
            self.main('rgb', '0000:1c:00.0', UUID)
        self.assertEqual([Path(r[0]).name for r in self.runs], ['rgb-deaktiver.sh'])
        self.assertFalse(config.exists())
        self.assertFalse(self.lock_held())

    def test_rgb_som_slaas_paa_igjen_for_laasen_stopper_installasjonen(self):
        self.permission('rgb', mode=0o600)
        run = self.fake_run

        def reactivated(argv, **kwargs):
            result = run(argv, **kwargs)
            if argv[0].endswith('rgb-deaktiver.sh'):
                self.journal.write_text('{}')
            return result

        self.mod['subprocess'] = type('S', (), {'run': reactivated, 'DEVNULL': -3, 'PIPE': -1})
        with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg'):
            self.main()
        self.assertEqual(len(self.runs), 1)

    def test_laasen_holdes_fra_kontroll_til_siste_aktivering(self):
        self.permission('nvml', pci='0000:01:00.0')
        self.permission('rgb', mode=0o600)
        self.assertEqual(self.main('rgb', '0000:1c:00.0', UUID), 0)
        # fake_run kontrollerer låsen for hvert trinn: ledig for de gamle
        # RGB-skriptene, holdt og arvet for installasjon og begge aktiveringene.
        self.assertEqual([Path(r[0]).name for r in self.runs],
                         ['rgb-deaktiver.sh', 'python3', 'aktiver.sh', 'rgb-aktiver.sh'])
        self.assertFalse(self.lock_held())

    def test_annen_administratorhandling_stopper_for_installasjonen(self):
        self.permission('nvml', pci='0000:01:00.0')
        with self.lock.open('wb') as other:
            fcntl.flock(other, fcntl.LOCK_EX)
            with self.assertRaisesRegex(SystemExit, 'annen administratorhandling'):
                self.main()
        self.assertEqual(self.runs, [])
        self.assertTrue(Path(self.mod['CONFIG']['nvml']).exists())
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_utrygg_laasefil_avvises(self):
        os.symlink(self.etc / 'mal', self.lock)
        with self.assertRaises(OSError):
            self.main()
        self.assertEqual(self.runs, [])

    # --- Arkivets identitet og festede summer (#19) ---

    def use(self, data):
        self.data = data
        self.assets[0]['digest'] = 'sha256:' + hashlib.sha256(data).hexdigest()

    def test_feil_identitet_eller_struktur_kjorer_ingenting(self):
        older = dict(FILES, **{'version.json': IDENTITY.replace(b'27', b'26')})
        other = dict(FILES, **{'version.json': IDENTITY.replace(b'1.5.2', b'1.5.1')})
        renamed = dict(FILES, **{'version.json': IDENTITY.replace(b'Agentic HW Monitor', b'Annet')})
        no_version = {k: v for k, v in FILES.items() if k != 'version.json'}
        link = tarfile.TarInfo('agentic-rigg-1.5.2/data/hjelper/rgb-admin.py')
        link.type, link.linkname = tarfile.SYMTYPE, '/etc/shadow'
        for data, error in ((archive(files=older), 'ikke Agentic HW Monitor 1.5.2 bygg 27'),
                            (archive(files=other), 'ikke Agentic HW Monitor 1.5.2 bygg 27'),
                            (archive(files=renamed), 'ikke Agentic HW Monitor'),
                            (archive(files=no_version), 'ikke Agentic HW Monitor'),
                            (archive(root='agentic-rigg-1.5.1'), 'uventet struktur'),
                            (archive(extra=(('annen/README', b'x'),)), 'uventet struktur'),
                            (archive(extra=(('agentic-rigg-1.5.2/../x', b'x'),)), 'uventet struktur'),
                            (archive(extra=(('agentic-rigg-1.5.2/version.json', IDENTITY),)), 'flere ganger|ugyldig systemmedlem'),
                            (archive(installer=None, files={k: v for k, v in FILES.items() if 'system-install' not in k}),
                             'ett gyldig installasjonsprogram')):
            self.use(data)
            with self.assertRaisesRegex(SystemExit, error):
                self.main()
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode='w:gz') as tar:
            for key, value in FILES.items():
                info = tarfile.TarInfo('agentic-rigg-1.5.2/' + key)
                info.size = len(value)
                tar.addfile(info, io.BytesIO(value))
            tar.addfile(link)
        self.use(buf.getvalue())
        with self.assertRaisesRegex(SystemExit, 'uventet struktur'):
            self.main()
        self.assertEqual(self.runs, [])
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_gammelt_appnavn_godtas_bare_for_utgivelser_for_navnebyttet(self):
        # Til og med bygg 32 het appen «Agentic Rigg». De utgivelsene skal
        # fortsatt kunne installeres, men et senere bygg må bruke det nye navnet.
        self.assertEqual((self.mod['NAME'], self.mod['OLD_NAME'], self.mod['OLD_NAME_LAST_BUILD']),
                         ('Agentic HW Monitor', 'Agentic Rigg', 32))
        known = self.mod['known_name']
        for name, build, ok in (('Agentic HW Monitor', 27, True), ('Agentic HW Monitor', 33, True),
                                ('Agentic Rigg', 27, True), ('Agentic Rigg', 32, True),
                                ('Agentic Rigg', 33, False), ('Agentic Rigg', 99, False),
                                ('agentic rigg', 27, False), ('Annet', 27, False), ('', 27, False),
                                (None, 27, False), (['Agentic Rigg'], 27, False)):
            self.assertEqual(known(name, build), ok, (name, build))

        def packed(name, version, build):
            identity = json.dumps({'name': name, 'version': version, 'channel': 'stable', 'build': build}).encode()
            path = self.staging / 'arkiv.tar.gz'
            path.write_bytes(archive(files=dict(FILES, **{'version.json': identity}), root=f'agentic-rigg-{version}'))
            return path

        verified = self.mod['verified_installer']
        for name, version, build in (('Agentic Rigg', '1.5.2', '27'), ('Agentic Rigg', '1.6.0', '32'),
                                     ('Agentic HW Monitor', '1.6.0', '32'), ('Agentic HW Monitor', '1.7.0', '33')):
            self.assertEqual(verified(packed(name, version, int(build)), version, build, None), b'print("installert")\n')
        with self.assertRaisesRegex(SystemExit, 'ikke Agentic HW Monitor 1.7.0 bygg 33'):
            verified(packed('Agentic Rigg', '1.7.0', 33), '1.7.0', '33', None)
        # Hele flyten: en utgivelse fra før navnebyttet installeres som før.
        old = dict(FILES, **{'version.json': IDENTITY.replace(b'Agentic HW Monitor', b'Agentic Rigg')})
        self.use(archive(files=old))
        self.assertEqual(self.main(), 0)
        self.assertEqual(len(self.runs), 1)

    def test_utpakket_storrelse_og_store_systemfiler_er_avgrenset(self):
        self.mod['MAX_UNPACKED'] = 100
        with self.assertRaisesRegex(SystemExit, 'utpakket'):
            self.main()
        self.mod['MAX_UNPACKED'] = 256 << 20
        big = dict(FILES, **{'version.json': IDENTITY + b' ' * 5000})
        for data, error in ((archive(installer=b'#' * ((1 << 20) + 1)), 'ett gyldig installasjonsprogram'),
                            (archive(files=big), 'ikke Agentic HW Monitor')):
            self.use(data)
            with self.assertRaisesRegex(SystemExit, error):
                self.main()
        self.assertEqual(self.runs, [])
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_festede_summer_avviser_utskiftet_utgivelse(self):
        good = pins()
        self.assertEqual(self.mod['main'](['1.5.2', '27', good]), 0)
        self.assertEqual(len(self.runs), 1)
        swapped = dict(FILES, **{'bin/agentic-rigg-helper': b'ondsinnet'})
        extra = dict(FILES, **{'data/hjelper/ekstra.sh': b'#!/bin/sh\n'})
        missing = {k: v for k, v in FILES.items() if k != 'data/hjelper/aktiver.sh'}
        for files, name in ((swapped, 'bin/agentic-rigg-helper'), (extra, 'data/hjelper/ekstra.sh'),
                            (missing, 'data/hjelper/aktiver.sh')):
            self.runs.clear()
            self.use(archive(files=files))
            with self.assertRaisesRegex(SystemExit, 'samsvarer ikke med utgivelsen.*' + name.replace('.', r'\.')):
                self.mod['main'](['1.5.2', '27', good])
            self.assertEqual(self.runs, [])
            self.assertEqual(list(self.staging.iterdir()), [])

    def test_ugyldige_festede_summer_stopper_for_nettet(self):
        good = pins()
        for bad in ('', 'x', good.replace('  ', ' '), good + good.splitlines(True)[0], good.upper(),
                    good.replace('version.json', 'data/icons/x.svg'), good.replace('bin/agentic-rigg-helper', 'bin/../x'),
                    ''.join(l for l in good.splitlines(True) if 'system-install' not in l), good * 30):
            with self.assertRaisesRegex(SystemExit, 'festede summer|mangler systemfiler'):
                self.mod['main'](['1.5.2', '27', bad])
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    # --- Bekreftelsen fra appen (#17) ---

    def answer_from(self, data, close=True, timeout=1):
        read, write = os.pipe()
        self.addCleanup(os.close, read)
        os.write(write, data)
        if close:
            os.close(write)
        else:
            self.addCleanup(os.close, write)
        return self.confirmation(timeout, read)

    def test_bekreftelsen_er_en_avgrenset_linje_med_tidsfrist(self):
        self.confirmation = runpy.run_path(str(DATA / 'system-hent.py'), run_name='test')['confirmation']
        ident = ('a' * 32).encode()
        self.assertEqual(self.answer_from(ident + b'\n'), 'a' * 32)
        self.assertEqual(self.answer_from(ident + b'\nrest'), 'a' * 32)
        for data in (b'', ident, b'x' * 80 + b'\n', 'æ'.encode() + b'\n'):
            self.assertIsNone(self.answer_from(data), data)
        self.assertIsNone(self.answer_from(b'', close=False, timeout=0.05))



    def test_pakkeskriptets_manifest_passer_hentingen(self):
        pack = runpy.run_path(str(DATA.parents[1] / 'scripts/pakk-utgivelse.py'), run_name='test')
        bin_dir = self.staging / 'bin'
        bin_dir.mkdir()
        for name in pack['BINARIES']:
            (bin_dir / name).write_bytes(name.encode())
        files = pack['file_list'](bin_dir)
        manifest = pack['system_manifest'](files).decode('ascii')
        pinned = self.mod['pinned'](manifest)
        by_name = {name: path for name, path, _ in files}
        self.assertEqual(pinned['bin/agentic-rigg-helper'], hashlib.sha256(b'agentic-rigg-helper').hexdigest())
        self.assertEqual(pinned['data/hjelper/system-install'],
                         hashlib.sha256((DATA / 'system-install').read_bytes()).hexdigest())
        # Nøyaktig de systemrelevante filene, også alle skriptene og policyene.
        expected = {name for name in by_name if name.startswith(('bin/', 'data/hjelper/')) or name == 'version.json'}
        self.assertEqual(set(pinned), expected)
        self.assertIn('data/hjelper/rgb-admin.py', pinned)
        self.assertNotIn('data/icons', manifest)


if __name__ == '__main__':
    unittest.main()
