# Notat fra grunnmuren til agentene i runde 2

Grunnmuragenten skrev dette etter runde 1, og hovedøkta har ført det inn her. Koden er fasit hvis noe ikke stemmer.

## Sider
- Hver sidefil eksporterer `pub fn build(app: &Rc<App>) -> Box<dyn Page>`.
- Alle åtte sidene er allerede registrert i `pages/mod.rs`.
- Trait-et `Page` har `widget`, `actions` (knappene i toppfeltet), `scrolls`, `show(query)`, `hide` og `on_key`.

## Innstillinger
- `settings/<x>.rs` eksporterer `pub fn build(app) -> Vec<Group>`.
- Hjelperne er `group`, `row`, `row_wrap`, `switch_row`, `seg`, `custom_row`, `save`, `request_render` og `live`.
- Søket finner nye rader av seg selv.

## Tilstand (`Rc<App>`)
- **Signaler:** `on_telemetry`, `on_settings(keys)`, `on_theme`, `on_devices`, `on_stats`, `on_conn`, `on_log`, `on_alarm`, `on_service` og `on_page`.
- **Lesing:** `gpu_sample()`, `sys()`, `value(key)`, `recent(key)` (de siste 62 s), `statics()` og `gpu_key(field)`.
- **Handlinger:** `set(json)`, `toast(Toast::new(..).body(..).action(..))`, `go(page)` og `open_settings(section)`.

## Klient og tjenesten
- Et kall går via `app.client.request(op, args, |res| …)`. Protokollen står i `docs/produksjon/protokoll.md`.
- `riggd/src/ops.rs` og `proto::READ_OPS` eies av hovedøkta. Trenger du en ny operasjon, skriver du det som et ønske i `LOGG.md`, eller lager den i en fil du eier og ber hovedøkta koble den inn.

## Logging og varsler (2B)
- **Tjenesten:** Du fyller ut `riggd/src/logg.rs` (`Logger`) og `riggd/src/varsler.rs` (`Alarms`). Tjenesten kaller allerede `on_sample` og `tick`.
- **Statiske NVML-data:** Data til Grafikkort-siden legges i `gpu/nvml_info.rs`. Øk `STEPS`, og skriv inn i `GpuStatic.extra`. Bruk bare metodene i `NvmlLib`, som alle er lesende.
- **Kroker i brukerflaten:** `logg_ui::attach` og `button`, `rapport_ui::about_row`, og `Sidebar::set_log_indicator`.

## Widgeter
- **Felles:** `Icon`, `Panel`, `Seg`, `Spark`, `KvList`, `Masonry`, `Fixed`, `Menu`/`popover`, `tag`, `dot`, `empty` og `infobar`.
- **2A** eier `widgets/chart.rs`, `gauge.rs` og `bar.rs`.
- **2C** eier `widgets/locked.rs`, `rgb.rs` og `lys3d/*`. Låsemerket settes med `Topbar::set_locked(page, true)`. `glow` og `glam` er allerede avhengigheter.

## Påskeegg
Påskeeggene ligger i `crate::easter`, med funksjonene `mark42`, `launch_day`, `filter_quip`, `curve_nickname`, `on_type_lys` og `aurora`.

## CSS
- Hver side har sin egen tomme fil i `app/data/theme/`, og den lastes allerede fra GResource.
- Fargene skal alltid komme fra variablene i `tokens.css`.

## Se resultatet
- `scripts/okt.sh -- <kommando>` starter en privat, usynlig økt med `gnome-shell --headless`. Den har egen HOME, egen sesjonsbuss og en tom systembuss.
- `scripts/bilder.sh` tar skjermbilder. Nye steg legges i `skjermbilder.rs` i debug-bygget.
- Bruk alltid `RIGG_PROFIL=dev`. Skriptene setter den for deg.
- Åpne aldri vinduer på brukerens ekte skrivebord.

## Ytelse
- Med Vulkan var kald start 410 ms og vinduet brukte 48–62 MB privat minne. Med `GSK_RENDERER=cairo` var tallene 160 ms og 10 MB.
- Runde 3 vurderer å bruke cairo som standard, med GL bare for 3D-kortet. Lag derfor grafer og widgeter som også ser skarpe og raske ut med cairo. Test med begge.
- Tegn bare når noe har endret seg, og stopp oppdateringen når siden er skjult.
