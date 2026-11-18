# Profiles (JavaScript)

## Purpose

Authoring format for maps: declarative calls into the draw.land library that build a `Grammar`.

## Key files

| Path | Role |
|------|------|
| `crates/drawland-script/src/library.js` | API surface profiles call |
| `crates/drawland-script/src/lib.rs` | Boa sandbox, natives, `evaluate()` / `evaluate_in(source, &Host)` / `evaluate_draft` |
| `crates/drawland-script/drawland.d.ts` | Type reference for editor + docs |
| `configs/*.js` | Example profiles |
| `crates/drawland/postcard.js` | The built-in default (`drawland::POSTCARD`) |

## Execution model

1. `library.js` is evaluated with natives (`dl.node`, `dl.fail`, …).
2. Profile source runs; each API call appends defs / draws / compose entries.
3. `evaluate` validates and returns `Grammar`. Interpreter is discarded before render.

Limits: `LOOP_LIMIT`, `RECURSION_LIMIT` in `lib.rs`. Errors format as `line N column M: …` using `SCRIPT_PATH` (`profile.js`).

## API groups (top-level)

`output`, `title`, `features`, `areas`, `lines`, `fills`, `engine`, `page`, `world`, `burin`, `ink`, `modules`, `draw`, `compose` — see `library_names()` / d.ts.

## The frame: `page` and `world`

What the script can ask while it runs, like a web page asks `window`: the
frame, never the data (see [decisions.md](decisions.md#scripts-read-the-frame-never-the-data)).

- `page.width`, `page.height`, `page.center` (mm), `page.frame` (the paper as an `Area`), `page.toWorld([x, y])` → `{ lat, lng }`.
- `world.scale`, `world.bearing`, `world.projection` (from the output alone), `world.center`, `world.bounds` (`{ north, south, east, west }` round the page's edge), `world.toPage({ lat, lng })` → `[x, y]`.

Natives `dl.sheet`, `dl.world`, `dl.toPage`, `dl.toWorld` answer from the innermost builder's output (or a view's keys when its style never called `output`), through `drawland_core::frame::Frame` (`frame.rs`): the output's projection, bearing and warp — the same `to_page` that places `at: { lat, lng }` in the engine. The frame is worked out once per output and cached on the builder. A `remap` warp (`warp: { remap: ([x, y]) => [x, y], grid }`) is a JS function, so `library.js`'s `output` samples it before the script goes on: it sets the output without warps, then for each remap sets the warps before it (`dl.setWarp`, which drops the cached frame) and calls the function on a `(grid + 1)²` lattice over the page, so `page.toWorld` inside it answers through the earlier warps. The samples go to the engine as a `mesh` warp; the graph never holds a function.

The place comes from the script's own `output({ lat, lng })`. A host that moves the map (the web page's card) writes the new place into that literal and runs the script again, so `world` is always the place drawn. The CLI's `--lat/--lng/--address` go in through `evaluate_in(source, &Host { place: Some((lat, lng)), .. })`, merged over `output()` like a view's keys. `scale`, `bearing` and `projection` need no place; the rest needs coordinates: when a script asks about an output that only names an `address`, the error carries `ScriptError::address`, and the CLI looks it up and runs the script again there.

Common patterns:

- `features.tiles("buildings")` / `features.osm({ … })`
- `features.json(url, { items, lat, lng })` — points from plain JSON (a live API), fields as dotted tags
- `features.geojson(url | path | {…})` — a dataset; properties are tags, so `features.where` bands it
- `features.contours({ step })` / `features.soundings({ every })` — the elevation as features, heights as tags (`ele`, `depth`, `t`, `index`); see [terrain-burin.md](terrain-burin.md)
- `areas.polygons`, `union`, `difference`, `grow`, `buffer`, …
- `areas.move(area, [across, down])`: the area moved across the page (a cloud's shadow cast off its footprint).
- `pattern: "clouds"` (penfill `clouds.rs`): cumulus from above, a covering fill like `isoline-stack`, so `fills.cover` gives its footprint. A cloud cover masks the map under it and stipples the footprint moved down-sun as the shadow.
- `fills.fill({ in, pattern, spacing, … })`
- `modules.roads({ from, widthMode, classes, … })`, `modules.labels({ from, rules })` — labels give `.lines`, `.area` (the ink, any font: grow it for clear paper round names) and `.dots` (the places' `marker` dots)
- `modules.graticule({ in, step, ticks, band, gap, size, font })`: an atlas plate's meridians and parallels round a page rectangle (`.lines`), its border keys (`.keys`), and degree labels (`.labels`, `.labelArea`). Its lines are traced and labelled in the engine; a script can draw its own with `world.toPage` (`configs/dead-reckoning.js`). The Schoolroom atlas uses the module, and picks its step from the plate's corners read back with `page.toWorld`.
- `burin.terrain((b) => …)` — burin-map-script builder; atlas stored on graph
- `compose.mask({ name, from, occludes: [draw ids…] })`
- `view({ id, at, size, lat, lng, scale }, "city-grain" | () => {…})` — another place drawn into a rectangle of the page. The script crate keeps a stack of builders: a function style builds the view's graph in place; a name asks the host for that style's script (`Host::styles`, a `Styles` closure: the CLI reads `name.js` beside the profile, the web page lends its presets; no host styles is an error) and runs it in a fresh interpreter. Inside a view, `output()` keeps the view's place and size over the style's own, so `page` and `world` are the view's.

## Authoring conventions

For substantial profiles (`configs/*.js`), follow `.cursor/rules/profile-style.mdc`:
- Group all visual constants, stroke specs, buffer widths, fill patterns, and taxonomy tables at the top (`1. PARAMETERS & STYLE PALETTE`).
- Execute feature extraction/geometry (`2`), `draw` declarations (`3`), `compose.mask` / `compose.order` (`4`), and plotter optimization (`5`) linearly below without inline magic numbers.
- One stroke width per pen; weight comes from offset double lines, never a wider stroke (`ink.width` is for print/screen styles such as the Schoolroom atlas).

## When changing the API

1. `library.js` + native bindings in `lib.rs`
2. `drawland.d.ts` (and tests that keep d.ts aligned)
3. Subsystem docs here if behaviour or contracts change
4. The web page's reference and Concepts (its own repository) are regenerated from `drawland.d.ts` there
