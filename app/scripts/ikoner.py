#!/usr/bin/env python3
"""Lager crates/rigg/src/widgets/ikoner_data.rs fra Lucide-symbolene i
web/index.html (ISC-lisens, se docs/LISENSER.md).

Hvert ikon blir én SVG-sti (rect, circle, ellipse og line gjøres om til
path-kommandoer), så vinduet kan tegne dem med GskPath og strek i tekstfargen,
akkurat som demoen (stroke-width 1,75 i et 24-rutenett).

Kjør: python3 -I scripts/ikoner.py
"""
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

HER = Path(__file__).resolve().parent.parent
HTML = HER.parent / 'web' / 'index.html'
UT = HER / 'crates' / 'rigg' / 'src' / 'widgets' / 'ikoner_data.rs'


def f(x):
    x = float(x)
    return ('%.3f' % x).rstrip('0').rstrip('.') if x != int(x) else str(int(x))


def circle(cx, cy, r):
    cx, cy, r = float(cx), float(cy), float(r)
    return f'M{f(cx - r)} {f(cy)}a{f(r)} {f(r)} 0 1 0 {f(2 * r)} 0a{f(r)} {f(r)} 0 1 0 {f(-2 * r)} 0Z'


def ellipse(cx, cy, rx, ry):
    cx, cy, rx, ry = map(float, (cx, cy, rx, ry))
    return f'M{f(cx - rx)} {f(cy)}a{f(rx)} {f(ry)} 0 1 0 {f(2 * rx)} 0a{f(rx)} {f(ry)} 0 1 0 {f(-2 * rx)} 0Z'


def rect(x, y, w, h, rx, ry):
    x, y, w, h = map(float, (x, y, w, h))
    rx = float(rx) if rx is not None else (float(ry) if ry is not None else 0)
    ry = float(ry) if ry is not None else rx
    rx, ry = min(rx, w / 2), min(ry, h / 2)
    if rx == 0:
        return f'M{f(x)} {f(y)}h{f(w)}v{f(h)}h{f(-w)}Z'
    return (f'M{f(x + rx)} {f(y)}h{f(w - 2 * rx)}a{f(rx)} {f(ry)} 0 0 1 {f(rx)} {f(ry)}'
            f'v{f(h - 2 * ry)}a{f(rx)} {f(ry)} 0 0 1 {f(-rx)} {f(ry)}h{f(-(w - 2 * rx))}'
            f'a{f(rx)} {f(ry)} 0 0 1 {f(-rx)} {f(-ry)}v{f(-(h - 2 * ry))}a{f(rx)} {f(ry)} 0 0 1 {f(rx)} {f(-ry)}Z')


# Lucide-ikoner appen bruker, men som demoen ikke har (samme form som i
# web/index.html). Demoen endres ikke.
EKSTRA = {
    # Loggknappen (produktspesifikasjonen 3.2).
    'circle-dot': '<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="1"/>',
    # Disker-siden (1.1.0, runde 6A).
    'hard-drive': '<line x1="22" x2="2" y1="12" y2="12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><line x1="6" x2="6.01" y1="16" y2="16"/><line x1="10" x2="10.01" y1="16" y2="16"/>',
}


def main():
    html = HTML.read_text(encoding='utf-8')
    out = []
    symbols = [(m.group(1), m.group(2)) for m in re.finditer(r'<symbol id="i-([a-z0-9-]+)" viewBox="0 0 24 24">(.*?)</symbol>', html, re.S)]
    have = {n for n, _ in symbols}
    symbols += [(n, b) for n, b in EKSTRA.items() if n not in have]
    for name, body in symbols:
        root = ET.fromstring(f'<g>{body}</g>')
        parts, dots = [], []
        for el in root:
            a = el.attrib
            if el.tag == 'path':
                parts.append(a['d'].strip())
            elif el.tag == 'circle':
                # Små fylte prikker (r ≤ 1, fill=currentColor) tegnes som prikker.
                if float(a['r']) <= 1:
                    dots.append(f"{a['cx']},{a['cy']},{a['r']}")
                parts.append(circle(a['cx'], a['cy'], a['r']))
            elif el.tag == 'ellipse':
                parts.append(ellipse(a['cx'], a['cy'], a['rx'], a['ry']))
            elif el.tag == 'rect':
                parts.append(rect(a.get('x', 0), a.get('y', 0), a['width'], a['height'], a.get('rx'), a.get('ry')))
            elif el.tag == 'line':
                parts.append(f"M{a['x1']} {a['y1']}L{a['x2']} {a['y2']}")
            else:
                sys.exit(f'Ukjent element {el.tag} i {name}')
        # Hver del tolkes for seg (en «m» først i en <path> er absolutt), så
        # de skilles med «|» og settes sammen i vinduet.
        out.append((name, '|'.join(parts)))
    out.sort()
    lines = [
        '// Generert av scripts/ikoner.py fra web/index.html. Ikke rediger for hånd.',
        '// Ikoner: Lucide (ISC-lisens, se docs/LISENSER.md).',
        '',
        '/// (navn, SVG-stier i et 24 × 24-rutenett, skilt med «|»).',
        'pub const ICONS: &[(&str, &str)] = &[',
    ]
    for name, d in out:
        lines.append(f'    ("{name}", "{d}"),')
    lines.append('];')
    UT.parent.mkdir(parents=True, exist_ok=True)
    UT.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{len(out)} ikoner til {UT}')


main()
