#!/usr/bin/python3
"""Prøver for språkverktøyene: flertallsregler, språkliste og katalogrevisjon."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parent
APP = SCRIPTS.parent


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


TOOL = load("i18n-trekk-ut")
AUDIT = load("i18n-revisjon")
NAME = "Agentic HW Monitor"


class Flertall(unittest.TestCase):
    def test_hvert_sprak_har_regel_og_gyldige_former(self):
        self.assertEqual(set(TOOL.RULES), set(TOOL.LANGS))
        self.assertEqual(len(TOOL.LANGS), len(set(TOOL.LANGS)))
        for lang in TOOL.LANGS:
            forms = {TOOL.plural_form(lang, n) for n in range(1000)}
            self.assertEqual(forms, set(range(TOOL.nplurals(lang))), lang)

    def test_kjente_regler(self):
        cases = {
            "sv": {0: 1, 1: 0, 2: 1, 21: 1}, "nb": {0: 1, 1: 0, 2: 1}, "fr": {0: 0, 1: 0, 2: 1},
            "pl": {0: 2, 1: 0, 2: 1, 5: 2, 12: 2, 22: 1, 101: 2}, "ja": {0: 0, 1: 0, 7: 0},
        }
        for lang, expected in cases.items():
            for n, form in expected.items():
                self.assertEqual(TOOL.plural_form(lang, n), form, (lang, n))

    def test_ugyldig_regel_avvises(self):
        original = dict(TOOL.RULES)
        self.addCleanup(lambda: (TOOL.RULES.clear(), TOOL.RULES.update(original)))
        for rule in ("nplurals=2; plural=(n != 1;", "nplurals=2; plural=(n + 1);",
                     "nplurals=2; plural=(n == 1 ? 0 : 5);", "nplurals=2; plural=(n != 1) 2;"):
            TOOL.RULES["sv"] = rule
            with self.assertRaises(TOOL.Invalid, msg=rule):
                TOOL.plural_form("sv", 3)


class Spraakliste(unittest.TestCase):
    def test_svensk_er_med_og_listene_er_like(self):
        self.assertIn("sv", TOOL.LANGS)
        self.assertEqual(TOOL.RULES["sv"], "nplurals=2; plural=(n != 1);")
        source = (APP / "crates/rigg-core/src/i18n/mod.rs").read_text(encoding="utf-8")
        self.assertEqual(AUDIT.rust_languages(source), list(TOOL.LANGS))
        self.assertEqual(set(AUDIT.FOREIGN_CHARS), set(TOOL.LANGS) - {"nb"})

    def test_avvik_mellom_all_og_code_oppdages(self):
        source = (APP / "crates/rigg-core/src/i18n/mod.rs").read_text(encoding="utf-8")
        with self.assertRaises(ValueError):
            AUDIT.rust_languages(source.replace('Self::Sv => "sv",', "", 1))


class Tekstregler(unittest.TestCase):
    def findings(self, lang, source, form, english=""):
        return AUDIT.text_findings(lang, source, form, NAME, english)

    def test_riktig_svensk_godtas(self):
        self.assertEqual(self.findings("sv", "Åpne Innstillinger", "Öppna Inställningar"), [])
        self.assertEqual(self.findings("sv", " · over gul grense", " · över gul gräns"), [])
        self.assertEqual(self.findings("sv", "5 × Ø 6 mm, kobber", "5 × Ø 6 mm, koppar"), [])

    def test_norske_tegn_og_fremmed_skrift_avvises(self):
        # Også når kilden har samme tegn: en uoversatt norsk tekst skal ikke slippe gjennom.
        self.assertEqual(self.findings("sv", "Kjører", "Kjører"), ["tegnet 'ø' hører ikke til sv"])
        self.assertEqual(self.findings("sv", "Læs", "Læs"), ["tegnet 'æ' hører ikke til sv"])
        self.assertEqual(self.findings("sv", "Ødelagt", "Ødelagt"), ["tegnet 'Ø' hører ikke til sv"])
        self.assertEqual(self.findings("en", "Åpne", "Åpne"), ["tegnet 'Å' hører ikke til en"])
        self.assertTrue(self.findings("sv", "Vifte", "Вентилятор"))
        self.assertTrue(self.findings("de", "Vifte", "Lüfter på"))
        self.assertEqual(self.findings("de", "Snitt", "Ø"), [])
        self.assertTrue(self.findings("en", "Vifte", "ファン"))

    def test_mellomrom_og_linjeskift_folger_kilden(self):
        self.assertTrue(self.findings("sv", " og ", "och"))
        self.assertTrue(self.findings("sv", "Linje\nto", "Rad två"))
        # Kinesisk og japansk skriver uten mellomrom, men linjeskift skal stå.
        self.assertEqual(self.findings("ja", " og ", "と"), [])
        self.assertTrue(self.findings("ja", "Linje\nto", "一行"))

    def test_tall_og_enhet_holdes_sammen(self):
        source = "kallet koster 25\u202fms og 0,85\u202f% av {tid}\u202fs"
        self.assertEqual(self.findings("sv", source, "anropet tar 25\u202fms och 0,85\u202f% av {tid}\u202fs"), [])
        for form in ("anropet tar 25 ms", "0,85 % av en kärna", "efter {tid} s"):
            self.assertEqual(self.findings("sv", source, form),
                             ["vanlig mellomrom mellom tall og enhet, bruk U+202F som i kilden"], form)
        # Engelsk og østasiatisk skrivemåte uten mellomrom er i orden.
        self.assertEqual(self.findings("en", source, "the call takes 25ms and 0.85% of {tid}s"), [])
        # Uten hardt mellomrom i kilden stilles det ikke krav.
        self.assertEqual(self.findings("sv", "Vent 5 s", "Vänta 5 s"), [])

    def test_eget_skriftsystem_kreves_for_setninger(self):
        self.assertTrue(self.findings("ja", "Skjermkortet svarer ikke", "GPU not responding"))
        self.assertEqual(self.findings("ja", "Synkronisert boost", "Sync Boost", "Sync boost"), [])
        self.assertEqual(self.findings("zh-CN", "Skjermkortet svarer ikke", "显卡无响应"), [])

    def test_appnavnet_star_uendret(self):
        self.assertEqual(self.findings("sv", f"Åpne {NAME}", f"Öppna {NAME}"), [])
        self.assertTrue(self.findings("sv", f"Åpne {NAME}", "Öppna Agentic Maskinvaruövervakare"))
        self.assertTrue(self.findings("sv", f"Åpne {NAME}", "Öppna Agentic Rigg"))
        path = "~/Dokumenter/Agentic Rigg"
        self.assertEqual(self.findings("sv", f"Tilbake til {path}", f"Tillbaka till {path}"), [])
        self.assertTrue(self.findings("sv", f"Tilbake til {path}", "Tillbaka till ~/Dokument/Agentic HW Monitor"))


class Skrivebordsfil(unittest.TestCase):
    TEXT = (APP / "data/io.github.AgenticRigg.desktop.in").read_text(encoding="utf-8")

    def test_alle_sprak_har_alle_felt(self):
        self.assertEqual(AUDIT.desktop_findings(self.TEXT, TOOL.LANGS, NAME), [])
        for key in ("Name[sv]=Agentic HW Monitor", "GenericName[sv]=", "Comment[sv]=", "Keywords[sv]=", "Name[sv]=Sensorer"):
            self.assertIn(key, self.TEXT)

    def test_manglende_sprak_feil_navn_og_ukjent_sprak_oppdages(self):
        without = "\n".join(line for line in self.TEXT.splitlines() if not line.startswith("Comment[sv]="))
        self.assertEqual(AUDIT.desktop_findings(without, TOOL.LANGS, NAME), ["Desktop Entry/Comment[sv] mangler"])
        old = self.TEXT.replace("Name[sv]=Agentic HW Monitor", "Name[sv]=Agentic Rigg")
        self.assertEqual(AUDIT.desktop_findings(old, TOOL.LANGS, NAME), ["Name[sv] skal være «Agentic HW Monitor», ikke «Agentic Rigg»"])
        extra = self.TEXT.replace("Comment[sv]=", "Comment[da]=x\nComment[sv]=")
        self.assertEqual(AUDIT.desktop_findings(extra, TOOL.LANGS, NAME), ["Desktop Entry/Comment[da] er et språk appen ikke har"])
        self.assertIn("Desktop Action sensorer/Name[sv] mangler",
                      AUDIT.desktop_findings(self.TEXT.replace("Name[sv]=Sensorer\n", ""), TOOL.LANGS, NAME))


class Kataloger(unittest.TestCase):
    def test_katalogene_har_ingen_funn(self):
        self.assertEqual(AUDIT.catalog_findings(), [])

    def test_hvert_sprak_er_fullstendig(self):
        index = TOOL.sources()
        totals = {lang: 0 for lang in TOOL.LANGS}
        for module in sorted(TOOL.catalog_modules()):
            entries = TOOL.entries_from_source(index[module])
            for lang in TOOL.LANGS:
                valid, missing = TOOL.check(lang, module, entries)
                self.assertEqual(missing, set(), (lang, module))
                totals[lang] += len(valid)
        self.assertGreater(totals["sv"], 3000)
        self.assertEqual(len(set(totals.values())), 1, totals)

    def test_revisjonen_som_kommando(self):
        result = subprocess.run([sys.executable, "-I", str(SCRIPTS / "i18n-revisjon.py"), "--check"],
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("0 katalogfunn", result.stderr)


if __name__ == "__main__":
    unittest.main()
