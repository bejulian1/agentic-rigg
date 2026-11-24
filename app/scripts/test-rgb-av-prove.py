#!/usr/bin/python3
"""rgb-av-prove.py mot den private RGB-fixturen (falsk-rgb.py): ingen
maskinvare, root eller vertens socket."""
import contextlib
import io
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent


class AvProveTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir='/tmp', prefix='rigg-av-')
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        root.chmod(0o700)
        marker = root / 'rgb-flow.fake'
        marker.write_text('agentic-rigg-rgb-fake-v1\n')
        marker.chmod(0o600)
        self.fake = subprocess.Popen([sys.executable, '-I', str(HERE / 'falsk-rgb.py'), '--root', str(root), '--build', '33'],
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.stop)
        self.socket = json.loads(self.fake.stdout.readline())['socket']
        self.mod = runpy.run_path(str(HERE / 'rgb-av-prove.py'), run_name='test')
        self.assertTrue(self.socket.startswith(str(root) + '/'), 'aldri vertens socket')

    def stop(self):
        self.fake.terminate()
        self.fake.wait(5)
        self.fake.stdout.close()
        self.fake.stderr.close()

    def main(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = self.mod['main']([*args, '--socket', self.socket, '--holdetid', '0.3'])
        return code, out.getvalue(), err.getvalue()

    def status(self):
        return self.mod['Hjelper'](self.socket).call('rgb_status')

    def test_hver_kandidat_skrives_holdes_og_rulles_tilbake_med_ett_sporsmal(self):
        base = self.status()['state']['confirmed']
        for number in (1, 2, 3, 4, 5):
            code, out, err = self.main('--kandidat', str(number))
            self.assertEqual(code, 0, err)
            self.assertEqual(out.count('SPØRSMÅL TIL BRUKEREN'), 1)
            state = self.status()['state']
            # Tilbake på utgangslyset, venter på et menneskes bekreftelse.
            self.assertEqual(state['confirmed'], base)
            self.assertTrue(state['recoveryNeedsConfirmation'])
            # Neste kandidat nektes til bekreftelsen er gitt.
            code, _, err = self.main('--kandidat', '2')
            self.assertEqual(code, 1)
            self.assertIn('--bekreft', err)
            code, out, _ = self.main('--bekreft')
            self.assertEqual(code, 0)
            state = self.status()['state']
            self.assertFalse(state['dirty'] or state['recoveryNeedsConfirmation'])

    def test_kandidatene_bruker_bare_faste_valg(self):
        base = [20, 40, 60]
        requests = {n: build(base) for n, (_, _, build) in self.mod['CANDIDATES'].items()}
        self.assertEqual(requests[1], {'mode': 'off'})
        self.assertEqual(requests[2], {'mode': 'static', 'color': [0, 0, 0], 'brightness': 0})
        self.assertEqual(requests[3], {'mode': 'static', 'color': base, 'brightness': 0})
        self.assertEqual(requests[4], {'mode': 'static', 'color': [1, 1, 1], 'brightness': 1})
        self.assertEqual(requests[5], requests[2], '«hold» prøver samme verdi, skrevet på nytt')

    def test_hold_kandidaten_skriver_paa_nytt_og_stopper_ved_tilbakerulling(self):
        writes = self.status()['holdWrites']
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = self.mod['main'](['--kandidat', '5', '--socket', self.socket, '--holdetid', '2.5'])
        self.assertEqual(code, 0, err.getvalue())
        status = self.status()
        self.assertGreaterEqual(status['holdWrites'], writes + 2, 'minst én ny skriving etter starten')
        self.assertFalse(status['keepalive'], '«hold» stopper med tilbakerullingen')
        self.assertTrue(status['state']['recoveryNeedsConfirmation'])

    def test_ctrl_c_gir_ingen_traceback(self):
        mod = self.mod
        def avbryt(*_):
            raise KeyboardInterrupt
        mod['run_candidate'].__globals__['time'].sleep, original = avbryt, mod['run_candidate'].__globals__['time'].sleep
        try:
            code, _, err = self.main('--kandidat', '2')
        finally:
            mod['run_candidate'].__globals__['time'].sleep = original
        self.assertEqual(code, 130)
        self.assertIn('--bekreft', err)
        self.assertNotIn('Traceback', err)

    def test_avbrutt_prove_rulles_tilbake_av_hjelperen(self):
        helper = self.mod['Hjelper'](self.socket)
        status = helper.call('rgb_status')
        helper.call('rgb_apply', {**self.mod['card'](status), 'request': {'mode': 'off'}, 'confirmTimeoutS': 30})
        helper.sock.close()  # skriptet dør midt i prøven
        deadline = time.monotonic() + 3
        while self.status()['state']['pending'] is not None and time.monotonic() < deadline:
            time.sleep(0.05)
        state = self.status()['state']
        self.assertIsNone(state['pending'])
        self.assertTrue(state['recoveryNeedsConfirmation'])

    def test_liste_og_ingen_root(self):
        code, out, _ = self.main('--liste')
        self.assertEqual(code, 0)
        self.assertIn('svart-0', out)
        self.assertIn('--kandidat N', out)


if __name__ == '__main__':
    unittest.main()
