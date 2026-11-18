# CLI and map data

## Purpose

The `drawland` crate (`crates/drawland`) is the host in this repository: geocoding, HTTP fetch, disk cache, then `grammar_run`. `Drawer::draw(profile, place, progress)` is the whole of it; the `drawland` binary (`drawland-cli`, a `main` over `drawland::cli::run`), the Python package and the Node package (`bindings/`) all call it. Another host (the draw.land web page) does its own fetching and hands the core the same bytes.

## Entry

`crates/drawland/src/cli.rs` (`cli::run(args)`, clap) — subcommand `map`:

- Profile: `.js` (evaluate script) or `.json` (saved graph)
- Place: `--address` (Nominatim) or `--lat` / `--lng`
- `--cache` (default `cache/`), `--offline`, `--refresh`
- `--dump-graph` — print JSON and exit
- SVG export: `--decimals <0..6>`, `--per-layer`, `--slices <whole|slices|both>` (`sheets::export_svg_files`)
- Sheets (`sheets.rs`): `output.slices` cuts the card into `columns × rows` sheets, each inset by `padding` on the card's outside edges and by `gutter / 2` on each side of every cut. With `slices.overlap`, the sheets stay one size and each runs `overlap` mm over its neighbour, n sheets of `(size + (n−1)·overlap) / n`, so neighbours carry the same strip of drawing. That lets a poster be printed in tiles: with no gutter the drawing runs to every cut, so with `overlap` at least twice the printer's unprintable edge, that edge trimmed off one sheet and laid on the next, the drawing runs on without a gap. The whole SVG crops once to the union of the sheets' inner rectangles (`Layout::overlaps`), so the shared strips are not drawn twice; each sheet's SVG has its own full copy.

`generate.sh` wraps common invocations.

## Library API (`crates/drawland/src/lib.rs`)

- `Profile::read(path)` (`.js` script with its folder, or `.json` graph), `Profile::script(source, dir)`, `Profile::postcard()` (`POSTCARD`, `crates/drawland/postcard.js`).
- `Place { address, lat, lng }`, `Settings { cache, offline, refresh }`.
- `Drawer::graph` runs the script only (an address is geocoded first, and again if the script asks `world` about its output's address); `Drawer::draw` = graph + `draw_graph` (locate, `gather` per page and per view, `run_with`). Styles a view names are `<name>.js` in the profile's folder.
- `Map { title, svg, groups, slices }`, `Map::files(ExportOptions)` / `Map::save(out, …)` (`sheets::export_svg_files`).
- Progress: any `Progress` (`&()` for none; `Term` paints the board on stderr).

## Bindings (`bindings/`)

- `bindings/python` (pyo3 abi3, maturin; module `drawland._drawland`, wrapper `python/drawland/`): `draw`, `graph`, `Map`, `POSTCARD`, `cli`; the `drawland` console script. `draw` runs with the GIL released.
- `bindings/node` (napi-rs 3; `native.js`/`native.d.ts` generated, `index.js`/`index.d.ts` the package's face, `cli.js` its bin): `draw` is an `AsyncTask` (a Promise), `graph` sync.
- Both are packaged by `.github/workflows/build.yml` for macOS arm64/x64, Linux x64/arm64 and Windows x64, with the sibling libraries from crates.io (`scripts/registry-deps.py`); `scripts/publish.sh` publishes crates.io, PyPI and npm from a run's artifacts.

## OSM / Overpass

`osm.rs` — parse Overpass JSON into `Feature`, tag matching via `select.rs`.

`osm_cache.rs` — request shaping: padded ground, cache keys, what a stored reply must cover. The same rules serve any host's cache.

HTTP: `crates/drawland/src/http.rs`. Policy constants (endpoints, timeouts) live in `output.rs`, for every host.

## Vector tiles

`tiles.rs` — terramask integration:

- Source: `TILES` URL (OpenFreeMap planet).
- Tile set from output bounds + zoom; building layers may force deeper zoom (`MAX_BUILDING_TILES` vs `MAX_FEATURE_TILES`).
- Features tagged with `tiles:layer` (`LAYER_TAG`).
- Ocean fetch may be skipped inland when Overpass shows no coastline (`filter_selectors`).

CLI downloads via terramask's fetcher; another host fetches the tiles and hands the bytes in.

## GeoJSON and JSON datasets

`features.json(url, { items, lat, lng })` compiles to `data_select: { json: { url, items, lat, lng } }` (`DataSource::Json`): plain JSON fetched and cached exactly like a GeoJSON URL (it is in `Grammar::datasets()`), then turned into points by `geojson::features_from_json` — dot-paths to the item array and each item's position, every other field a tag under its dotted path. First user: `configs/rush-hour.js` (MBTA vehicles).


`features.geojson(source)` compiles to `data_select`. A string source (URL or file path) is listed by `Grammar::datasets()`; the host reads each one and passes the text as `Inputs::data`, keyed as written. An inline object (`{ geojson: {…} }`) lives in the graph and needs no host.

- Core: `geojson.rs` turns GeoJSON into `Feature`s (`Element::Data`) in lon/lat: `properties` become tags (numbers written out, `null` dropped), and a point's third coordinate becomes the tag `geom:z` (a quake's depth). The runner projects them like the tiles; a missing dataset is a plan error.
- CLI: `read_datasets` in `main.rs`. URLs go through `Http::dataset` and are cached in `<cache>/data/<sha1>.geojson` for `DATA_MAX_AGE_S` (15 min, so live feeds stay current); `--refresh` re-downloads and `--offline` takes any age. Other sources are files relative to the profile.
- Web mercator clamps latitude to ±85.05°, so a world dataset (Antarctica) projects. World maps use a PROJ string (`+proj=moll`); proj4rs has no Equal Earth.

## Terrain tiles

Terrarium PNGs for elevation — terramask. Consumed by `terrain.rs` / burin.

## Google fonts

Fetch: `crates/drawland/src/fonts.rs` (`fetch_google`). Core: `google_fonts.rs`, `text.rs`.

## Stroke fonts (labels / `lines.text`)

`crates/drawland/src/fonts.rs` (`fetch_stroke`) fetches catalog or custom URLs from `stroke_font_keys_in_grammar`. Core: `stroke_font.rs`, `text.rs`.

## Cache layout

Default `cache/overpass/` for Overpass bodies; geocoding under cache root.

## No I/O in core

`drawland-core` accepts in-memory features, tile bytes, and font bytes. Any new data source should either feed those buffers or extend `tiles.rs` / `osm.rs`, not add HTTP to core.

## Views

A page with `views` (`view(...)` in a profile) is placeless unless the profile or command line gives a place. `map_v3` gathers the page's inputs and each view's (`gather`: Overpass, tiles, terrain, fonts, datasets for that view's output), each as its own `download <id>` row. The web app does the same in the worker (see [web.md](web.md)).
