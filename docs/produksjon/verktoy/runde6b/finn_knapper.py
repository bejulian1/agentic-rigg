"""Runde 6B: finner vindusknappene i et utsnitt av toppfeltet (høyre side).

Leter etter de tre ikonene fra høyre: kolonner der ikonfargen skiller seg
tydelig fra bakgrunnen i midtre høyde. Skriver «x y» for midten av hver
knapp, fra høyre (lukk, maksimer, minimer).
"""
import sys
from PIL import Image

im = Image.open(sys.argv[1]).convert("RGB")
w, h = im.size
# Bakgrunnen: fargen som er vanligst i den midtre høyden (hjørnene kan vise skrivebordet).
band = [im.getpixel((x, y)) for x in range(w) for y in range(h // 4, 3 * h // 4)]
bg = max(set(band), key=band.count)


def diff(p):
    return sum(abs(a - b) for a, b in zip(p, bg))


# Kolonner med tydelig avvik (ikon eller sirkel) i den midtre halvdelen.
cols = []
for x in range(w):
    m = max(diff(im.getpixel((x, y))) for y in range(h // 4, 3 * h // 4))
    cols.append(m > 90)
runs, start = [], None
for x, on in enumerate(cols + [False]):
    if on and start is None:
        start = x
    elif not on and start is not None:
        runs.append((start, x - 1))
        start = None
runs = [r for r in runs if r[1] - r[0] >= 6]
for a, b in list(reversed(runs))[:3]:
    print((a + b) // 2, h // 2)
