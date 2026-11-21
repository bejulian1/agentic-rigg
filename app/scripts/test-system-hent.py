#!/usr/bin/python3
"""Prøver for system-hent.py uten nett, root eller systemfiler."""
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
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
        patcher = patch('os.geteuid', return_value=0)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.mod['open_url'] = self.open_url
        self.mod['subprocess'] = type('S', (), {'run': self.fake_run, 'DEVNULL': -3})

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
            raise urllib.error.HTTPError(request.full_url, 404, 'Not Found', {}, None)
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
