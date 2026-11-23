# Lisenser i Agentic HW Monitor

Agentic HW Monitor 1.0 er to programmer: vinduet (`agentic-rigg`) og tjenesten (`agentic-riggd`). Begge er skrevet i Rust og lisensiert under MIT (teksten står nederst). Programmene laster ikke ned noe og sender ingenting ut av maskinen.

Denne fila følger med i programmet (Innstillinger › Om › Lisenser) og installeres til `~/.local/share/doc/agentic-rigg/LISENSER.md`.

## Ikoner: Lucide (ISC)

Vinduet bruker 72 ikoner fra [Lucide](https://lucide.dev) (lucide-static 1.60.0). Bare stiene er kopiert inn i programmet. Appikonet er tegnet for prosjektet og bruker fargene fra GNOME-paletten (Adwaita).

Ikonene: activity, arrow-down, arrow-down-up, arrow-left, arrow-right, arrow-up, cake, chart-area, chart-line, check, chevron-down, chevron-right, chevrons-down-up, chevrons-up-down, circle-alert, circle-check, circle-dot, circle-help, circle-slash, copy, cpu, crosshair, database, download, egg, ellipsis-vertical, eye, eye-off, fan, folder, gpu, grip-vertical, info, keyboard, layout-dashboard, lock, memory-stick, minus, monitor, moon, network, palette, panel-left, panel-left-close, panel-left-open, pause, pencil, pin, pin-off, play, plus, refresh-cw, rotate-ccw, rotate-cw, ruler, save, search, settings, shield, sliders-horizontal, sun, sun-moon, thermometer, timer, trash-2, triangle-alert, undo-2, unplug, upload, wifi, x, zap.

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

Noen Lucide-ikoner bygger på Feather (MIT). Av dem bruker appen arrow-down, arrow-right, arrow-up, check, chevron-down, chevron-right, download, info, lock, minus, monitor, moon, plus, search, trash-2, upload og x.

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

## Rust-biblioteker som er bygget inn

Alle er lisensiert under MIT, eller under MIT og Apache 2.0 etter eget valg. Agentic HW Monitor bruker dem under MIT. glow kan også brukes under Zlib, og memchr under Unlicense. unicode-ident har i tillegg Unicode-3.0 for tabellene.

| Bibliotek | Versjon | Lisens |
|---|---|---|
| async-channel | 2.5.0 | Apache-2.0 OR MIT |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| cairo-rs | 0.22.9 | MIT |
| cairo-sys-rs | 0.22.9 | MIT |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| event-listener | 5.4.2 | Apache-2.0 OR MIT |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT |
| field-offset | 0.3.6 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| gdk-pixbuf | 0.22.0 | MIT |
| gdk-pixbuf-sys | 0.22.9 | MIT |
| gdk4 | 0.11.5 | MIT |
| gdk4-sys | 0.11.5 | MIT |
| gio | 0.22.10 | MIT |
| gio-sys | 0.22.9 | MIT |
| glam | 0.30.10 | MIT OR Apache-2.0 |
| glib | 0.22.10 | MIT |
| glib-sys | 0.22.9 | MIT |
| glib-unix | 0.22.8 | MIT |
| glib-unix-sys | 0.22.9 | MIT |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib |
| gobject-sys | 0.22.9 | MIT |
| graphene-rs | 0.22.8 | MIT |
| graphene-sys | 0.22.9 | MIT |
| gsk4 | 0.11.5 | MIT |
| gsk4-sys | 0.11.5 | MIT |
| gtk4 | 0.11.5 | MIT |
| gtk4-sys | 0.11.5 | MIT |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| memoffset | 0.9.1 | MIT |
| pango | 0.22.9 | MIT |
| pango-sys | 0.22.9 | MIT |
| parking | 2.2.1 | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| smallvec | 1.16.5 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| toml_datetime | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | 0.25.19+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| winnow | 1.0.5 | MIT |
| zmij | 1.0.23 | MIT |

## Systembiblioteker

Disse følger med Ubuntu og lastes ved kjøring. De er ikke en del av Agentic HW Monitor:

- GTK 4, GLib, Pango, cairo, graphene, GdkPixbuf og HarfBuzz (LGPL 2.1 eller nyere, og MPL 1.1 eller LGPL 2.1 for cairo).
- `libnvidia-ml.so.1` (NVML) følger med NVIDIA-driveren og brukes bare av tjenesten.

## MIT-lisensen

Gjelder Agentic HW Monitor og Rust-bibliotekene over. Opphavsretten til hvert bibliotek tilhører forfatterne som er oppført i biblioteket.

```
The MIT License (MIT)

Copyright (c) 2026 Agentic HW Monitor-prosjektet

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
