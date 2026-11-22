#!/usr/bin/python3
"""Prøver for system-hent.py uten nett, root eller systemfiler."""
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


def archive(installer=b'print("installert")\n', extra=()):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode='w:gz') as tar:
        for name, data in (('agentic-rigg-1.5.2/data/hjelper/system-install', installer), *extra):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return buf.getvalue()


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
        self.journal = self.etc / 'journal.json'
        self.mod['RGB_JOURNAL'] = str(self.journal)
        uid = patch.dict(os.environ, {'PKEXEC_UID': '1000'})
        uid.start()
        self.addCleanup(uid.stop)

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
                    return type('R', (), {'returncode': 0, 'stdout': self.inspect})()
                if argv[0].endswith('rgb-avklar.sh'):
                    self.journal.unlink()
                return type('R', (), {'returncode': 0})()
            if argv[0].endswith('rgb-deaktiver.sh'):
                # Etter kontrollen av arkivet, før installasjonen.
                self.assertEqual(len(list(self.staging.iterdir())), 1)
                return type('R', (), {'returncode': self.deactivate_code})()
            self.assertEqual(list(self.staging.iterdir()), [])
            return type('R', (), {'returncode': 0})()
        self.assertEqual(Path(argv[-3]).read_bytes(), b'print("installert")\n')
        self.assertEqual(hashlib.sha256(Path(argv[-2]).read_bytes()).hexdigest(), argv[-1])
        self.assertEqual(oct(os.stat(Path(argv[-3]).parent).st_mode & 0o777), '0o700')
        self.assertEqual(kwargs['env'], {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
        return type('R', (), {'returncode': 0})()

    def test_henter_kontrollerer_og_installerer(self):
        self.assertEqual(self.mod['main'](['1.5.2', '27']), 0)
        self.assertEqual(self.urls[0], 'https://api.github.com/repos/bejulian1/agentic-rigg/releases/tags/v1.5.2')
        self.assertEqual(self.runs[0][:3], ['/usr/bin/python3', '-I', '-S'])
        self.assertEqual(self.runs[0][-1], self.digest)
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_installerer_og_aktiverer_med_ett_passord(self):
        pci, uuid = '0000:1C:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        with patch.dict(os.environ, {'PKEXEC_UID': '1000'}):
            self.assertEqual(self.mod['main'](['1.5.2', '27', 'nvml', pci, uuid]), 0)
            self.assertEqual(self.runs[-1], ['/usr/local/libexec/agentic-rigg/aktiver.sh', '--pci', pci.lower(), '--uuid', uuid])
            self.assertEqual(self.mod['main'](['1.5.2', '27', 'rgb', pci, uuid]), 0)
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
        self.assertEqual(self.mod['main'](['1.5.2', '27', 'rgb', '0000:1c:00.0', uuid]), 0)
        self.assertEqual(self.runs[0], ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-deaktiver.sh', '--pci', '0000:1c:00.0', '--uuid', uuid])
        self.assertEqual(self.runs[1][:3], ['/usr/bin/python3', '-I', '-S'])
        self.assertEqual(self.runs[2], ['/usr/local/libexec/agentic-rigg/aktiver.sh', '--pci', '0000:01:00.0', '--uuid', other])
        self.assertEqual(self.runs[3][0], '/usr/local/libexec/agentic-rigg-rgb-v2/rgb-aktiver.sh')
        # Ber brukeren selv om GPU-styring, aktiveres den bare én gang.
        self.runs.clear()
        self.assertEqual(self.mod['main'](['1.5.2', '27', 'nvml', '0000:1c:00.0', uuid]), 0)
        self.assertEqual([r[0] for r in self.runs], ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-deaktiver.sh', '/usr/bin/python3',
                                                     '/usr/local/libexec/agentic-rigg/aktiver.sh'])
        self.assertEqual(self.runs[-1][2], '0000:1c:00.0')

    def test_uavklart_rgb_stopper_for_installasjonen(self):
        self.permission('rgb', mode=0o600)
        self.deactivate_code = 1
        self.assertEqual(self.mod['main'](['1.5.2', '27']), 1)
        self.assertEqual(len(self.runs), 1)
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_uavklart_okt_avsluttes_med_brukerens_bekreftelse(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        self.inspect = json.dumps({'expected': {'mode': 'static', 'color': [1, 2, 3], 'brightness': 80},
                                   'recoveryId': 'a' * 32}).encode()
        self.assertEqual(self.mod['main'](['1.5.2', '27', 'rgb', '0000:1c:00.0', uuid]), 0)
        ids = ['--pci', '0000:1c:00.0', '--uuid', uuid]
        lib = '/usr/local/libexec/agentic-rigg-rgb-v2/'
        self.assertEqual(self.runs[:4], [[lib + 'rgb-gjenopprett.sh', *ids, '--restore-original'],
                                         [lib + 'rgb-avklar.sh', *ids, '--inspect'],
                                         [lib + 'rgb-avklar.sh', *ids, '--static-confirmed', '--recovery-id', 'a' * 32],
                                         [lib + 'rgb-deaktiver.sh', *ids]])
        self.assertEqual(self.runs[-1][0], lib + 'rgb-aktiver.sh')

    def test_uavklart_okt_feil_trinn_stopper_resten(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.inspect = json.dumps({'expected': {'mode': 'off'}, 'recoveryId': 'b' * 32}).encode()
        lib = '/usr/local/libexec/agentic-rigg-rgb-v2/'
        for step, ran in (('rgb-gjenopprett', 1), ('inspect', 2), ('rgb-avklar', 3)):
            self.runs.clear()
            self.journal.write_text('{}')
            self.fail_at = step
            self.assertEqual(self.mod['main'](['1.5.2', '27', 'rgb', '0000:1c:00.0', uuid]), 1)
            self.assertEqual(len(self.runs), ran, step)
            self.assertTrue(all(r[0].startswith(lib) and 'deaktiver' not in r[0] for r in self.runs))
            self.assertEqual(list(self.staging.iterdir()), [])
        # Tom rapport: ingen avklaring, men deaktivering og installasjon.
        self.fail_at, self.inspect = None, b'{}'
        self.runs.clear()
        self.assertEqual(self.mod['main'](['1.5.2', '27', 'rgb', '0000:1c:00.0', uuid]), 0)
        self.assertEqual([Path(r[0]).name for r in self.runs[:4]],
                         ['rgb-gjenopprett.sh', 'rgb-avklar.sh', 'rgb-deaktiver.sh', 'python3'])

    def test_uavklart_okt_og_feil_sum_rorer_ikke_kortet(self):
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        self.assets[0]['digest'] = 'sha256:' + '0' * 64
        with self.assertRaisesRegex(SystemExit, 'Kontrollsummen stemmer ikke'):
            self.mod['main'](['1.5.2', '27', 'rgb', '0000:1c:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01'])
        self.assertEqual(self.runs, [])

    def test_uavklart_okt_uten_bekreftelse_stopper_for_nettet(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        other = 'GPU-87654321-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        # GPU-styring, ingen aktivering, eller et annet kort enn RGB-tillatelsen.
        for extra in (['nvml', '0000:1c:00.0', uuid], [], ['rgb', '0000:1c:00.0', other]):
            with self.assertRaisesRegex(SystemExit, 'kortet 0000:1c:00.0 .*åpne RGB-lys'):
                self.mod['main'](['1.5.2', '27', *extra])
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_ugyldig_avklaringsmal_beholder_journalen(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        for report in (b'{"recoveryId": "../x"}', b'[]', b'ikke json'):
            self.runs.clear()
            self.inspect = report
            with self.assertRaisesRegex(SystemExit, 'ugyldig avklaringsmål'):
                self.mod['main'](['1.5.2', '27', 'rgb', '0000:1c:00.0', uuid])
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
                self.mod['main'](['1.5.2', '27'])
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_eldre_tillatelse_for_samme_bruker_faar_egen_melding(self):
        path = self.etc / 'helper.json'
        path.write_text(json.dumps({'versjon': 1, 'uid': 1000}))
        path.chmod(0o600)
        with self.assertRaisesRegex(SystemExit, 'eldre oppsett'):
            self.mod['main'](['1.5.2', '27'])
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_aktivering_bare_etter_vellykket_installasjon(self):
        def failing(argv, **kwargs):
            self.runs.append(argv)
            return type('R', (), {'returncode': 3})()
        self.mod['subprocess'] = type('S', (), {'run': failing, 'DEVNULL': -3})
        with patch.dict(os.environ, {'PKEXEC_UID': '1000'}):
            self.assertEqual(self.mod['main'](['1.5.2', '27', 'nvml', '0000:1c:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01']), 3)
        self.assertEqual(len(self.runs), 1)

    def test_ugyldig_aktivering_stopper_for_nettet(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        for extra, env in ((['gpu', '0000:1c:00.0', uuid], '1000'), (['nvml', '1c:00.0', uuid], '1000'),
                           (['nvml', '0000:1c:00.0', 'GPU-x'], '1000'), (['nvml', '0000:1c:00.0', uuid], ''),
                           (['nvml', '0000:1c:00.0', uuid], '0'), (['rgb', '0000:1c:00.0', uuid, '--x'], '1000')):
            with patch.dict(os.environ, {'PKEXEC_UID': env}), self.assertRaises(SystemExit):
                self.mod['main'](['1.5.2', '27', *extra])
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    def test_feil_sum_kjorer_ingenting(self):
        self.assets[0]['digest'] = 'sha256:' + '0' * 64
        with self.assertRaisesRegex(SystemExit, 'Kontrollsummen stemmer ikke'):
            self.mod['main'](['1.5.2', '27'])
        self.assertEqual(self.runs, [])
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_mangler_arkiv_eller_sum(self):
        for assets in ([], [{'name': NAME}], [{'name': NAME, 'digest': 'sha512:' + 'a' * 64}], self.assets * 2):
            self.assets = assets
            with self.assertRaises(SystemExit):
                self.mod['main'](['1.5.2', '27'])
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
            self.mod['main'](['1.5.2', '27'])
        def broken(request, hosts):
            raise http.client.IncompleteRead(b'')
        self.mod['open_url'] = broken
        with self.assertRaisesRegex(SystemExit, 'Fikk ikke kontakt med GitHub'):
            self.mod['main'](['1.5.2', '27'])

    def test_ugyldige_argumenter_og_ikke_root(self):
        for args in ([], ['1.5', '27'], ['１.５.２', '27'], ['1.5.2', '0'], ['1.5.2;x', '27'], ['1.5.2', '27', 'x'], ['../1.5.2', '27']):
            with self.assertRaisesRegex(SystemExit, 'VERSJON BYGG'):
                self.mod['main'](args)
        with patch('os.geteuid', return_value=1000), self.assertRaisesRegex(SystemExit, 'administrator'):
            self.mod['main'](['1.5.2', '27'])
        self.assertEqual(self.urls, [])

    def test_for_stort_arkiv_og_tvetydig_installasjon(self):
        self.mod['MAX_ARCHIVE'] = 10
        with self.assertRaisesRegex(SystemExit, 'større enn forventet'):
            self.mod['main'](['1.5.2', '27'])
        self.mod['MAX_ARCHIVE'] = 64 << 20
        self.data = archive(extra=(('annen/data/hjelper/system-install', b'x'),))
        self.assets[0]['digest'] = 'sha256:' + hashlib.sha256(self.data).hexdigest()
        with self.assertRaisesRegex(SystemExit, 'ett gyldig installasjonsprogram'):
            self.mod['main'](['1.5.2', '27'])
        self.assertEqual(self.runs, [])
        self.assertEqual(list(self.staging.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
