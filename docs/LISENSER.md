# Lisenser for tredjepartsinnhold

Agentic Rigg har ingen avhengigheter som lastes ned ved kjøring. Det eneste tredjepartsinnholdet er ikoner. Appikonet (`web/icons/`, `assets/icons/`) er tegnet for prosjektet i v0.0.6 og bruker bare fargene fra GNOME-paletten (Adwaita).

## Lucide-ikoner (lucide-static 1.60.0)

64 ikoner fra [Lucide](https://lucide.dev) er kopiert inn som `<symbol>`-elementer i en SVG-sprite øverst i `web/index.html`. De brukes med `<svg class="icon"><use href="#i-navn"/></svg>`. Ingen CDN, og alt virker uten nett. Bare stiene er kopiert; strekbredden (1,75) og fargen settes i `web/styles.css`.

Ikoner som brukes: layout-dashboard, thermometer, chart-line, gpu, cpu, sliders-horizontal, fan, palette, settings, menu, copy, check, pause, play, rotate-ccw, undo-2, chevrons-down-up, chevrons-up-down, chevron-down, chevron-right, trash-2, save, eye, eye-off, info, triangle-alert, circle-alert, circle-check, zap, memory-stick, arrow-down-up, arrow-down, arrow-up, lock, circle-slash, plus, search, x, pencil, keyboard, download, upload, folder, refresh-cw, unplug, monitor, sun, moon, rotate-cw, circle-help, arrow-right, activity, wifi, network, ellipsis-vertical, minus, sun-moon, ruler, panel-left, chart-area, database, pin, pin-off, grip-vertical (de tre siste fra v0.0.6).

```
ISC License

Copyright (c) 2026 Lucide Icons and Contributors

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

Noen Lucide-ikoner bygger på Feather (MIT). Av dem bruker appen: check, chevron-down, chevron-right, arrow-down, arrow-up, arrow-right, download, upload, info, lock, minus, monitor, moon, plus, search, trash-2, x.

```
The MIT License (MIT)

Copyright (c) 2013-present Cole Bemis

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
