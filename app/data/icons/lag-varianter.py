#!/usr/bin/env python3
"""Lager ikonvariantene til Agentic HW Monitor fra én mal per størrelse.

Malene ligger i varianter/maler/ (128.svg er hovedikonet, 48/32/24/16.svg er
tegnet på pikselrutenettet). Hver variant er bare et sett farger. Skriptet
skriver

  varianter/<id>/{128,48,32,24,16}.svg alle varianter
  varianter/varianter.json             liste for appen (id, navn, ikonnavn, rekkefølge)
  hicolor/scalable/apps/<app>.svg      standardvarianten (hovedikonet)
  hicolor/{16x16,24x24,32x32,48x48}/apps/<app>.svg

Bruk:
  ./lag-varianter.py                 skriv alt på nytt
  ./lag-varianter.py --sjekk         skriv ingenting, avslutt med 1 hvis noe er utdatert
  ./lag-varianter.py --forhandsvis DIR
                                     lag PNG-forhåndsvisninger i DIR (krever
                                     python3-gi med GdkPixbuf og Pillow)

Fargene for aksentvariantene er de samme som --accent-bg, --a-l og --a-d i
web/styles.css. Endres de der, endres de her.
"""
import json
import sys
from pathlib import Path

APP = 'io.github.AgenticRigg'
HER = Path(__file__).resolve().parent
MALER = HER / 'varianter' / 'maler'
STORRELSER = ('128', '48', '32', '24', '16')
HICOLOR = {'128': 'scalable', '48': '48x48', '32': '32x32', '24': '24x24', '16': '16x16'}

GULL = {'kontakt': '#e5a50a', 'kontakt-kant': '#c88800'}
GRAA_KONTAKT = {'kontakt': '#9a9996', 'kontakt-kant': '#77767b'}

# Grafitt, som standardtemaet i appen. Måleren og navet har aksentfargen.
GRAFITT = {
    'flate-topp': '#55555b', 'flate': '#424247', 'kant': '#2a2a2e',
    'glans-farge': '#ffffff', 'glans': '.14',
    'spor': '#2e2e32', 'spor-dekning': '1',
    'bronn': '#2e2e32', 'bronn-dekning': '1', 'blad': '#f6f5f4',
    **GULL,
}

# (id, navn, --accent-bg, --a-l, --a-d) fra web/styles.css, seksjon 2.
AKSENTER = [
    ('blue', 'Blå', '#2a75cf', '#2169be', '#559cf7'),
    ('teal', 'Blågrønn', '#138093', '#017284', '#4da8bb'),
    ('green', 'Grønn', '#308540', '#257736', '#5bac66'),
    ('purple', 'Lilla', '#9141ac', '#8a3da4', '#c481dc'),
    ('pink', 'Rosa', '#b95083', '#a84576', '#e677ab'),
    ('slate', 'Skifer', '#5f7184', '#536475', '#8a9daf'),
]


def bland(a, b, t):
    """Blander to #rrggbb-farger, t = andel av b."""
    pa = [int(a[i:i + 2], 16) for i in (1, 3, 5)]
    pb = [int(b[i:i + 2], 16) for i in (1, 3, 5)]
    return '#' + ''.join(f'{round(x + (y - x) * t):02x}' for x, y in zip(pa, pb))


def aksentflis(bg, lys):
    """Flisen i aksentfargen, hvit måler og hvite blad."""
    return {
        'flate-topp': bland(bg, '#ffffff', .14), 'flate': bg, 'kant': bland(bg, '#000000', .4),
        'glans-farge': '#ffffff', 'glans': '.18',
        'spor': '#000000', 'spor-dekning': '.22', 'maler': '#ffffff',
        'bronn': '#000000', 'bronn-dekning': '.3', 'blad': '#ffffff',
        'nav': bg, 'nav-lys': lys, **GULL,
    }


def varianter():
    v = [('standard', 'Standard', 'Grafitt med blå måler',
          {**GRAFITT, 'maler': '#2a75cf', 'nav': '#2a75cf', 'nav-lys': '#559cf7'})]
    for vid, navn, bg, _lys_tema, mork_tema in AKSENTER:
        v.append((vid, navn, f'Flis i {navn.lower()}', aksentflis(bg, mork_tema)))
    v.append(('dark', 'Mørk', 'Nesten svart flis', {
        'flate-topp': '#2a2a2e', 'flate': '#1c1c1f', 'kant': '#0c0c0e',
        'glans-farge': '#ffffff', 'glans': '.12',
        'spor': '#3a3a3f', 'spor-dekning': '1', 'maler': '#559cf7',
        'bronn': '#000000', 'bronn-dekning': '.45', 'blad': '#deddda',
        'nav': '#2a75cf', 'nav-lys': '#559cf7', **GULL}))
    v.append(('light', 'Lys', 'Lys flis, mørk vifte', {
        'flate-topp': '#ffffff', 'flate': '#f0eff1', 'kant': '#bcbcc2',
        'glans-farge': '#000000', 'glans': '.07',
        'spor': '#000000', 'spor-dekning': '.1', 'maler': '#2169be',
        'bronn': '#3d3d42', 'bronn-dekning': '1', 'blad': '#ffffff', 'skive': '#3d3d42',
        'nav': '#2169be', 'nav-lys': '#559cf7', **GULL}))
    v.append(('mono', 'Monokrom', 'Bare gråtoner', {
        **GRAFITT, 'maler': '#deddda', 'nav': '#9a9996', 'nav-lys': '#deddda', **GRAA_KONTAKT}))
    return v


def fyll(mal, navn, farger):
    # Skiven midt i 16 px-ikonet (der bladene ikke får plass) har bladfargen,
    # med mindre varianten sier noe annet.
    farger = {'skive': farger['blad'], **farger}
    ut = mal.replace('{{navn}}', navn)
    for k, verdi in farger.items():
        ut = ut.replace('{{' + k + '}}', verdi)
    if '{{' in ut:
        rest = ut[ut.index('{{'):].split('}}')[0] + '}}'
        raise SystemExit(f'Mangler farge for {rest} i varianten «{navn}»')
    return ut


def filer():
    """Alle filene skriptet eier, som {sti: innhold}."""
    maler = {s: (MALER / f'{s}.svg').read_text(encoding='utf-8') for s in STORRELSER}
    ut = {}
    liste = []
    for vid, navn, beskrivelse, farger in varianter():
        for s in STORRELSER:
            ut[HER / 'varianter' / vid / f'{s}.svg'] = fyll(maler[s], navn, farger)
        ikonnavn = APP if vid == 'standard' else f'{APP}-{vid}'
        liste.append({'id': vid, 'navn': navn, 'beskrivelse': beskrivelse, 'ikonnavn': ikonnavn})
        if vid == 'standard':
            for s in STORRELSER:
                ut[HER / 'hicolor' / HICOLOR[s] / 'apps' / f'{APP}.svg'] = ut[HER / 'varianter' / vid / f'{s}.svg']
    ut[HER / 'varianter' / 'varianter.json'] = json.dumps(
        {'app': APP, 'standard': 'standard', 'storrelser': {s: HICOLOR[s] for s in STORRELSER}, 'varianter': liste},
        ensure_ascii=False, indent=2) + '\n'
    return ut


def skriv(sjekk):
    utdatert = []
    for sti, innhold in filer().items():
        if sti.exists() and sti.read_text(encoding='utf-8') == innhold:
            continue
        utdatert.append(sti.relative_to(HER))
        if not sjekk:
            sti.parent.mkdir(parents=True, exist_ok=True)
            sti.write_text(innhold, encoding='utf-8')
    if sjekk:
        for s in utdatert:
            print(f'Utdatert: {s}')
        return 1 if utdatert else 0
    print(f'Skrev {len(utdatert)} filer.' if utdatert else 'Alt var oppdatert.')
    return 0


# ---------------------------------------------------------------- forhåndsvisning

def forhandsvis(mappe):
    import gi
    gi.require_version('GdkPixbuf', '2.0')
    from gi.repository import GdkPixbuf
    from PIL import Image, ImageDraw, ImageFont

    mappe = Path(mappe)
    mappe.mkdir(parents=True, exist_ok=True)
    lyst, mork = ('#fafafb', '#1e1e21'), ('#222226', '#e8e8eb')

    def ikon(sti, px):
        pb = GdkPixbuf.Pixbuf.new_from_file_at_size(str(sti), px, px)
        if not pb.get_has_alpha():
            pb = pb.add_alpha(False, 0, 0, 0)
        return Image.frombuffer('RGBA', (pb.get_width(), pb.get_height()), pb.get_pixels(),
                                'raw', 'RGBA', pb.get_rowstride(), 1).copy()

    def skrift(px, fet=False):
        navn = 'Ubuntu-B.ttf' if fet else 'Ubuntu-R.ttf'
        for sti in (f'/usr/share/fonts/truetype/ubuntu/{navn}',
                    '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf' if fet else
                    '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'):
            try:
                return ImageFont.truetype(sti, px)
            except OSError:
                pass
        return ImageFont.load_default()

    def beste(vid, px):
        """Filen GNOME ville valgt: håndtegnet 16/24/32, ellers den skalerbare."""
        return HER / 'varianter' / vid / (f'{px}.svg' if str(px) in HICOLOR and px != 128 else '128.svg')

    def lim(bilde, im, x, y):
        bilde.paste(im, (x, y), im)

    # 1. Størrelsesstigen 16–512 på lyst og mørkt
    trinn = (16, 24, 32, 48, 64, 128, 256, 512)
    bredde = 40 + sum(max(p, 48) + 32 for p in trinn)
    hoyde = 512 + 100
    bilde = Image.new('RGB', (bredde, hoyde * 2))
    t = ImageDraw.Draw(bilde)
    for rad, (bg, fg) in enumerate((lyst, mork)):
        y0 = rad * hoyde
        t.rectangle([0, y0, bredde, y0 + hoyde], fill=bg)
        x = 32
        for p in trinn:
            lim(bilde, ikon(beste('standard', p), p), x, y0 + 40 + 512 - p)
            t.text((x, y0 + 40 + 512 + 16), f'{p} px', fill=fg, font=skrift(14))
            x += max(p, 48) + 32
    bilde.save(mappe / 'storrelser.png', optimize=True)

    # 2. Alle varianter, 48 og 128 px, på lyst og mørkt
    alle = varianter()
    kol = 150
    bredde = 32 + kol * len(alle)
    hoyde = 128 + 48 + 110
    bilde = Image.new('RGB', (bredde, hoyde * 2))
    t = ImageDraw.Draw(bilde)
    for rad, (bg, fg) in enumerate((lyst, mork)):
        y0 = rad * hoyde
        t.rectangle([0, y0, bredde, y0 + hoyde], fill=bg)
        for i, (vid, navn, _b, _f) in enumerate(alle):
            x = 32 + i * kol
            lim(bilde, ikon(beste(vid, 128), 128), x, y0 + 24)
            lim(bilde, ikon(beste(vid, 48), 48), x + 40, y0 + 24 + 128 + 16)
            tekst = navn
            w = t.textlength(tekst, font=skrift(15, True))
            t.text((x + 64 - w / 2, y0 + 24 + 128 + 16 + 48 + 14), tekst, fill=fg, font=skrift(15, True))
    bilde.save(mappe / 'varianter.png', optimize=True)

    # 3. Pikselnivå: skalert 128-mal mot håndtegnet 16/24/32/48, forstørret ×6
    z = 6
    trinn = (16, 24, 32, 48)
    bredde = 32 + sum(p * z + 32 for p in trinn) * 2
    hoyde = 48 * z + 90
    bilde = Image.new('RGB', (bredde, hoyde * 2))
    t = ImageDraw.Draw(bilde)
    for rad, (bg, fg) in enumerate((lyst, mork)):
        y0 = rad * hoyde
        t.rectangle([0, y0, bredde, y0 + hoyde], fill=bg)
        x = 32
        for p in trinn:
            for kilde, merke in ((HER / 'varianter' / 'standard' / '128.svg', 'skalert'), (beste('standard', p), 'håndtegnet')):
                im = ikon(kilde, p).resize((p * z, p * z), Image.NEAREST)
                lim(bilde, im, x, y0 + 50 + (48 - p) * z)
                t.text((x, y0 + 16), f'{p} px {merke}', fill=fg, font=skrift(14))
                x += p * z + 32
    bilde.save(mappe / 'finpuss-16-48.png', optimize=True)

    # 4. På lys og mørk bakgrunn i GNOME-størrelser: dash (64), appnett (96), Alt+Tab (128)
    #    sammen med Agentic Klipp, så familien kan sammenlignes.
    klipp = HER.parents[3] / 'klipp' / 'data' / 'icons' / 'hicolor' / 'scalable' / 'apps' / 'io.github.AgenticKlipp.svg'
    flater = [('Lyst vindu', '#fafafb', '#1e1e21'), ('Mørkt vindu', '#242424', '#ffffff'),
              ('Oversikt (mørk)', '#16161a', '#ffffff'), ('Dash (grå)', '#38383d', '#ffffff')]
    kol = 96 + 64 + 64 + 64
    bredde = 32 + kol * len(flater)
    hoyde = 96 + 64 + 120
    bilde = Image.new('RGB', (bredde, hoyde))
    t = ImageDraw.Draw(bilde)
    for i, (navn, bg, fg) in enumerate(flater):
        x0 = i * kol
        t.rectangle([x0, 0, x0 + kol, hoyde], fill=bg)
        t.text((x0 + 24, 16), navn, fill=fg, font=skrift(14, True))
        lim(bilde, ikon(beste('standard', 96), 96), x0 + 24, 50)
        if klipp.exists():
            lim(bilde, ikon(klipp, 96), x0 + 24 + 96 + 24, 50)
        x = x0 + 24
        for vid in ('standard', 'dark', 'light'):
            lim(bilde, ikon(beste(vid, 48), 48), x, 50 + 96 + 24)
            x += 48 + 16
        lim(bilde, ikon(beste('standard', 32), 32), x, 50 + 96 + 24 + 8)
        lim(bilde, ikon(beste('standard', 16), 16), x + 40, 50 + 96 + 24 + 16)
    bilde.save(mappe / 'bakgrunner.png', optimize=True)

    # 5. Skissene, hvis de finnes
    skisser = sorted((mappe / 'skisser').glob('*.svg')) if (mappe / 'skisser').is_dir() else []
    if skisser:
        trinn = (16, 24, 32, 48, 64, 128)
        lab = 190
        bredde = 32 + lab + sum(max(p, 48) + 28 for p in trinn) + 16 * 6 + 32 + 24 * 6 + 32
        radh = 128 + 30
        hoyde = 44 + radh * len(skisser)
        bilde = Image.new('RGB', (bredde, hoyde * 2))
        t = ImageDraw.Draw(bilde)
        for rad, (bg, fg) in enumerate((lyst, mork)):
            y0 = rad * hoyde
            t.rectangle([0, y0, bredde, y0 + hoyde], fill=bg)
            x = 32 + lab
            for p in trinn:
                t.text((x, y0 + 14), f'{p} px', fill=fg, font=skrift(12))
                x += max(p, 48) + 28
            t.text((x, y0 + 14), '16 og 24 px ×6', fill=fg, font=skrift(12))
            for j, sti in enumerate(skisser):
                y = y0 + 44 + j * radh
                navn = sti.stem.split('-', 1)
                t.text((32, y + 56), f'{navn[0].upper()}  {navn[1].replace("-", " ").capitalize()}', fill=fg, font=skrift(15, True))
                x = 32 + lab
                for p in trinn:
                    lim(bilde, ikon(sti, p), x, y + 128 - p)
                    x += max(p, 48) + 28
                lim(bilde, ikon(sti, 16).resize((96, 96), Image.NEAREST), x, y + 32)
                lim(bilde, ikon(sti, 24).resize((144, 144), Image.NEAREST), x + 96 + 16, y - 8)
        bilde.save(mappe / 'skisser.png', optimize=True)

    print(f'Forhåndsvisninger i {mappe}')


def main(argv):
    if '-h' in argv or '--help' in argv:
        print(__doc__)
        return 0
    if '--forhandsvis' in argv:
        i = argv.index('--forhandsvis')
        if i + 1 >= len(argv):
            raise SystemExit('--forhandsvis trenger en mappe')
        if skriv(sjekk=True):
            print('Kjør skriptet uten flagg først, så forhåndsvisningene viser det som ligger på disk.')
            return 1
        forhandsvis(argv[i + 1])
        return 0
    return skriv(sjekk='--sjekk' in argv)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
