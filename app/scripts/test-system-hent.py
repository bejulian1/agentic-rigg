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
IDENTITY = json.dumps({'name': 'Agentic HW Monitor', 'version': '1.5.2', 'channel': 'stable', 'build': 27}).encode()
FILES = {'version.json': IDENTITY, 'bin/agentic-rigg-helper': b'hjelper', 'data/hjelper/system-install': b'print("installert")\n',
         'data/hjelper/agentic-rigg-card.service': b'unit', 'data/hjelper/aktiver.sh': b'#!/bin/sh\n', 'README.md': b'les meg', 'data/icons/x.svg': b'<svg/>'}

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
        self.etc.mkdir(mode=0o700)
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

        uid = patch.dict(os.environ, {'PKEXEC_UID': '1000'})
        uid.start()
        self.addCleanup(uid.stop)

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
            self.assertIn(argv[0], (self.mod['ACTIVATE'], self.mod['DEACTIVATE']))
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
            self.assertEqual(self.main('card', pci, uuid), 0)
            self.assertEqual(self.runs[-1], ['/usr/local/libexec/agentic-rigg/aktiver.sh', '--card-control', '--pci', pci.lower(), '--uuid', uuid])
        self.assertEqual(len(self.runs), 2)

    def permission(self, domain, uid=1000, pci='0000:1c:00.0', uuid='GPU-12345678-abcd-ef01-2345-6789abcdef01', mode=0o644):
        card = {'pci': pci, 'uuid': uuid}
        data = {'version': 1, 'uid': uid, 'enabled': True, 'binding': card, 'build': 28} if domain == 'rgb' else \
            {'versjon': 2, 'uid': uid, 'enabled': True, **card, 'bygg': 28}
        path = Path(self.mod['CONFIG'][domain])
        path.write_text(json.dumps(data))
        path.chmod(mode)

    def test_revoke_legacy_with_one_approval(self):
        self.permission('rgb', uuid='GPU-23456789-abcd-ef01-2345-6789abcdef01', mode=0o600)
        self.permission('nvml')
        self.journal.write_text('unresolved')
        self.assertEqual(self.main('revoke-card'), 0)
        self.assertEqual([Path(r[0]).name for r in self.runs], ['python3', 'deaktiver.sh'])
        self.assertEqual(self.journal.read_text(), 'unresolved')

    def test_migrate_legacy_permissions_with_one_activation(self):
        for build in (32, 33):
            for domains in [('nvml',), ('rgb',), ('nvml', 'rgb')]:
                with self.subTest(build=build, domains=domains):
                    for path in self.mod['CONFIG'].values():
                        Path(path).unlink(missing_ok=True)
                    for domain in domains:
                        self.permission(domain)
                        path = Path(self.mod['CONFIG'][domain])
                        auth = json.loads(path.read_text())
                        auth['bygg' if domain == 'nvml' else 'build'] = build
                        path.write_text(json.dumps(auth))
                    self.journal.write_text('unresolved')
                    self.runs.clear()
                    self.assertEqual(self.main('card', '0000:1c:00.0', UUID), 0)
                    self.assertEqual([Path(r[0]).name for r in self.runs], ['python3', 'aktiver.sh'])
                    self.assertEqual(self.journal.read_text(), 'unresolved')

    def test_nedgradering_stopper_for_nedlasting_og_rgb_endringer(self):
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        self.marker.write_text('2:28\n')
        self.marker.chmod(0o644)
        with self.assertRaisesRegex(SystemExit, 'nedgradere.*28.*27'):
            self.main('card', '0000:1c:00.0', uuid)
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
            self.main('card', '0000:1c:00.0', uuid)
        self.assertEqual(len(self.urls), 2)
        self.assertEqual(self.runs, [])
        self.assertTrue(self.journal.exists())
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_uavklart_okt_og_feil_sum_rorer_ikke_kortet(self):
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        self.assets[0]['digest'] = 'sha256:' + '0' * 64
        with self.assertRaisesRegex(SystemExit, 'Kontrollsummen stemmer ikke'):
            self.main('card', '0000:1c:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01')
        self.assertEqual(self.runs, [])

    def test_uavklart_okt_for_annet_rgb_kort_stopper_for_nettet(self):
        other = 'GPU-87654321-abcd-ef01-2345-6789abcdef01'
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        with self.assertRaisesRegex(SystemExit, 'autoriserte kortet'):
            self.main('card', '0000:1c:00.0', other)
        self.assertEqual(self.urls, [])
        self.assertEqual(self.runs, [])

    # --- GPU-styring med en uavklart RGB-økt (1.6.0-feilen) ---

    def test_uten_rgb_installeres_alt_som_for(self):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(self.main('card', '0000:1c:00.0', UUID), 0)
        self.assertNotIn('AGENTIC-RIGG-RGB-BEHOLDT', out.getvalue())

    def test_beholdt_rgb_som_endres_for_laasen_stopper_installasjonen(self):
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')
        lock = self.mod['SystemLock']
        journal = self.journal

        class Changed(lock):
            def acquire(self):
                journal.unlink()  # Avklart av noen andre rett før låsen tas.
                return super().acquire()

        self.mod['SystemLock'] = Changed
        with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg'):
            self.main('card', '0000:1c:00.0', UUID)
        self.assertEqual(self.runs, [])
        self.assertFalse(self.lock_held())
        self.assertEqual(list(self.staging.iterdir()), [])

    def test_beholdt_rgb_som_endres_under_nedlasting_stopper(self):
        self.permission('rgb', mode=0o600)
        self.journal.write_text('{}')

        def settled_during_download(request, hosts):
            response = self.open_url(request, hosts)
            if request.full_url.startswith('https://github.com/'):
                self.journal.unlink()
            return response

        self.mod['open_url'] = settled_during_download
        with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg'):
            self.main('card', '0000:1c:00.0', UUID)
        self.assertEqual(self.runs, [])

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
            self.assertEqual(self.main('card', '0000:1c:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01'), 3)
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
        for extra in (['card', '0000:01:00.0', UUID],):
            self.permission('nvml', pci='0000:01:00.0')
            with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg'):
                self.main(*extra)
            self.assertEqual(self.runs, [])
            self.assertFalse(config.exists())
            self.assertEqual(list(self.staging.iterdir()), [])
            self.assertFalse(self.lock_held())

    def test_endret_tillatelse_under_nedlasting_stopper_for_rgb_endringer(self):
        self.permission('rgb', mode=0o600)
        self.permission('nvml')

        def rebound_during_download(request, hosts):
            response = self.open_url(request, hosts)
            if request.full_url.startswith('https://github.com/'):
                self.permission('nvml', pci='0000:02:00.0')
            return response

        self.mod['open_url'] = rebound_during_download
        with self.assertRaisesRegex(SystemExit, 'Tillatelsene endret seg'):
            self.main('card', '0000:1c:00.0', UUID)
        self.assertEqual(self.runs, [])

    def test_laasen_holdes_fra_kontroll_til_siste_aktivering(self):
        self.permission('nvml')
        self.permission('rgb', mode=0o600)
        self.assertEqual(self.main('card', '0000:1c:00.0', UUID), 0)
        # fake_run kontrollerer låsen for hvert trinn: ledig for de gamle
        # RGB-skriptene, holdt og arvet for installasjon og begge aktiveringene.
        self.assertEqual([Path(r[0]).name for r in self.runs],
                         ['python3', 'aktiver.sh'])
        self.assertFalse(self.lock_held())

    def test_annen_administratorhandling_stopper_for_installasjonen(self):
        self.permission('nvml', pci='0000:01:00.0')
        with self.lock.open('wb') as other:
            fcntl.flock(other, fcntl.LOCK_EX)
            with self.assertRaisesRegex(SystemExit, 'annen administratorhandling'):
                self.main('card','0000:01:00.0',UUID)
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
