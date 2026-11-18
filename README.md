# draw.land

Turn any address into a pen-plotter postcard map. OpenStreetMap data is
fetched, projected onto the paper and drawn as distinct layers (buildings
hatched, parks dotted, water waved, roads at their real width). The output is
one layered SVG in paper millimetres; what is drawn, and how, is a short
JavaScript profile. Area patterns come from
[penfill](https://github.com/piLeoni/penfill).

The profile can also prepare the strokes for the machine (merge, simplify,
sort, a hand-drawn tremble). Sending the SVG to a plotter is left to the
plotter's own software.

The map can be laid flat in web Mercator, UTM or any PROJ projection (a pole
at the centre of the page, for one), or in a projection the profile writes
itself as a function of the page (`warp: { remap }`).

This repository is the engine and its command line. The web page at
[draw.land](https://draw.land), with its editor, its presets and the
documentation at [draw.land/docs](https://draw.land/docs), is built on it; the
styles in its gallery are plain profiles, so any of them can be copied from
there and drawn here.

## Use

The command line installs from crates.io:

```bash
cargo install drawland-cli
drawland map --address "Piazza Maggiore, Bologna" -o map.svg
```

With no profile it draws the postcard, an example built in
([crates/drawland-cli/postcard.js](crates/drawland-cli/postcard.js)). From a
checkout:

```bash
./generate.sh "1730 Highland Pl, Berkeley, CA 94709"
# → exports/1730_highland_pl_berkeley_ca_94709/map.svg
```

or directly:

```bash
cargo run --release -p drawland-cli -- map --address "Piazza Maggiore, Bologna" -o map.svg
cargo run --release -p drawland-cli -- map configs/contours.js --lat 46.5 --lng 11.85
```

| Argument | Does |
|---|---|
| `profile` | A profile script (`.js`) or a saved graph (`.json`); default `postcard`, the example built in |
| `--address` / `--lat --lng` | Where; overrides the profile's `output({...})` |
| `--dump-graph` | Print the graph the profile builds, as JSON, and stop |
| `-o` | Output SVG (default `map.svg`) |
| `--cache` | Overpass / Nominatim response cache (default `cache/`; Overpass replies in `cache/overpass/`, reused by any map they cover) |
| `--offline` | Use cached responses only |
| `--refresh` | Fetch datasets again even when cached |
| `--slices` | For a card cut into sheets: `whole`, `slices` or `both` (default) |
| `--per-layer` | Also one SVG per top-level layer (a file per pen) |

## Profiles

A profile is a JavaScript file that calls the draw.land library: where the
map is and on what paper (`output({...})`), which map data to read
(`features.tiles("buildings")`, `features.osm({ leisure: "park" })`), what to
make of it (`areas.*`, `lines.*`, `fills.fill`, `modules.roads()`), and how to
ink and stack it (`draw`, `compose`). The script only builds a graph; the
engine runs the graph in Rust. A script can ask about its frame as it runs —
`page` (the paper) and `world` (the ground under it: centre, scale, bounds,
`world.toPage({ lat, lng })`, `page.toWorld([x, y])`) — but never about the
data: heights and the like come back as features with tags
(`features.contours`, `features.soundings`).

A profile can draw another place into a rectangle of its page in a style
named by its script: `view({ id, at, size, lat, lng, scale }, "city-grain")`
reads `city-grain.js` beside the profile.

`crates/drawland-script/drawland.d.ts` is the library's exact description: an
editor that knows TypeScript autocompletes and type-checks a profile against
it (`// @ts-check` with the file in a `jsconfig.json`).

## Layout

| Path | Role |
|---|---|
| `crates/drawland-core` | The graph and its runner, projection, OSM and tiles → features, fills, compose, SVG. No I/O. |
| `crates/drawland-script` | Profile scripts: the library and its JavaScript engine (Boa), script → graph |
| `crates/drawland-cli` | The `drawland` binary: geocoding, Overpass, vector tiles, cache |
| `configs/` | Example profiles |
| `docs/architecture/` | Architecture index for contributors and coding agents (`AGENTS.md`) |
| `reference/python/` | The former Python pipeline, kept to read, not run |

## Tests

```bash
cargo test
```

The pattern types in `drawland.d.ts` are written from penfill's catalogue:
`cargo run -p drawland-script --example write_dts` (`cargo test` fails when
they are stale).

draw.land builds against sibling checkouts of its own libraries, developed
alongside it: penfill in `../penfill` (fills), terramask in `../terramask`
(map tiles as polygons, the sea, elevation bands), burin-map in
`../burin-map` (pen-and-ink terrain) and deoverlap in `../deoverlap`.

## License

AGPL-3.0-only.
