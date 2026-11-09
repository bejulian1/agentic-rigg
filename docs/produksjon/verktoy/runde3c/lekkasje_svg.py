#!/usr/bin/env python3
"""Lager kurven fra lekkasjetesten (lekkasje.csv) som SVG, og skriver et sammendrag.

Bruk: python3 -I lekkasje_svg.py lekkasje.csv runde3c-lekkasje.svg
"""
import csv
import sys


def main(src, dst):
    rows = list(csv.DictReader(open(src)))
    t = [int(r["s"]) / 60 for r in rows]
    pd = [int(r["tjeneste_private_dirty_kb"]) / 1024 for r in rows]
    pss = [int(r["tjeneste_pss_kb"]) / 1024 for r in rows]
    win = [r["vindu"] == "1" for r in rows]
    wpd = [int(r["vindu_private_dirty_kb"]) / 1024 if r["vindu_private_dirty_kb"] else None for r in rows]
    fd = sorted({r["fd"] for r in rows})
    thr = sorted({r["traader"] for r in rows})
    ps = sorted({r["pstate"] for r in rows})

    # Stigning (minste kvadrater) for tjenestens private minne, MB per time, etter første 5 min.
    pts = [(x, y) for x, y in zip(t, pd) if x >= 5]
    n = len(pts)
    mx = sum(x for x, _ in pts) / n
    my = sum(y for _, y in pts) / n
    slope = sum((x - mx) * (y - my) for x, y in pts) / max(1e-9, sum((x - mx) ** 2 for x, _ in pts)) * 60
    print(f"prøver {len(rows)}, {t[-1]:.0f} min, vinduet åpnet {sum(1 for a, b in zip([False] + win, win) if b and not a)} ganger")
    print(f"tjenesten Private_Dirty: første {pd[0]:.1f} MB, etter 5 min {pts[0][1]:.1f}, siste {pd[-1]:.1f}, min {min(pd):.1f}, maks {max(pd):.1f}, stigning {slope:+.2f} MB/t")
    print(f"tjenesten Pss: første {pss[0]:.1f}, siste {pss[-1]:.1f}; fildeskriptorer {fd}; tråder {thr}; P-tilstand {ps}")
    wv = [w for w in wpd if w is not None]
    if wv:
        print(f"vinduet Private_Dirty mens det var åpent: {min(wv):.1f}–{max(wv):.1f} MB")

    W, H, L, R, T, B = 760, 300, 52, 16, 20, 36
    xmax = max(t)
    ymax = max(max(pd), 30) * 1.15

    def X(x):
        return L + x / xmax * (W - L - R)

    def Y(y):
        return T + (1 - y / ymax) * (H - T - B)

    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" font-family="sans-serif" font-size="11">',
           f'<rect width="{W}" height="{H}" fill="#ffffff"/>']
    # Vinduet åpent: grå felt.
    start = None
    for x, w in zip(t + [t[-1]], win + [False]):
        if w and start is None:
            start = x
        if not w and start is not None:
            out.append(f'<rect x="{X(start):.1f}" y="{T}" width="{X(x) - X(start):.1f}" height="{H - T - B}" fill="#e8eef7"/>')
            start = None
    step = 5 if ymax < 40 else 10
    y = 0
    while y <= ymax:
        out.append(f'<line x1="{L}" x2="{W - R}" y1="{Y(y):.1f}" y2="{Y(y):.1f}" stroke="#e2e2e2"/><text x="{L - 6}" y="{Y(y) + 4:.1f}" text-anchor="end" fill="#555">{y} MB</text>')
        y += step
    for m in range(0, int(xmax) + 1, 10):
        out.append(f'<text x="{X(m):.1f}" y="{H - B + 16}" text-anchor="middle" fill="#555">{m} min</text>')
    poly = " ".join(f"{X(x):.1f},{Y(v):.1f}" for x, v in zip(t, pd))
    out.append(f'<polyline points="{poly}" fill="none" stroke="#1c71d8" stroke-width="1.75"/>')
    poly = " ".join(f"{X(x):.1f},{Y(v):.1f}" for x, v in zip(t, pss))
    out.append(f'<polyline points="{poly}" fill="none" stroke="#9141ac" stroke-width="1.25" stroke-dasharray="4 3"/>')
    out.append(f'<text x="{L}" y="13" fill="#222">Tjenesten i 1 t (2 s-takt, ekte kort): privat minne (blå) og Pss (lilla, stiplet). Blått felt: vinduet åpent. Stigning {slope:+.2f} MB/t.</text>')
    out.append("</svg>")
    open(dst, "w").write("\n".join(out))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
