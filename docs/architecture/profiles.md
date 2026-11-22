# The library and its faces (JavaScript, TypeScript, Python, Node, Rust)

## Purpose

A style is declarative calls into the draw.land library that build a `Grammar`. The library is written once, in Rust (`drawland-graph`): every function, check, default and message. JavaScript and TypeScript profiles, Python, Node and Rust are faces of it, equal: the same calls give the same graph (tests compare them node for node). A **profile** is the JavaScript or TypeScript form, the shareable one (the web page's editor and gallery).

## Key files

| Path | Role |
|------|------|
| `crates/drawland-graph/src/library.rs` | The library: `Graph::call(name, args)` for every function in `FUNCTIONS`, `get_property` for `PROPERTIES`, `get` for a module's lazy parts |
| `crates/drawland-graph/src/graph.rs` | The graph being built: nodes, draws, compose, output and frame, the view stack, `finish` |
| `crates/drawland-graph/src/arg.rs` | `Arg`, the values every face passes (handles, `Lazy` parts, `Func` marks), and their marked JSON (`encode` / `decode`) |
| `crates/drawland-graph/src/api.rs` | The typed Rust face: `g.areas().union(…)`, `Features` / `Area` / `Lines`, `obj!` |
| `crates/drawland-script/src/library.js` | The JavaScript face: a function of `dl` (the library's calls) returning the namespaces. Boa installs it as a profile's globals; the Node package runs the same file over its `NativeGraph` |
| `crates/drawland-script/src/lib.rs` | Boa sandbox and natives over one thread-local `Graph`: `evaluate()` / `evaluate_in(source, &Host)` / `evaluate_draft`, `run_into(graph, source, host)` (a JS style into a graph built elsewhere) |
| `crates/drawland-ts` | TypeScript: `strip_types` blanks the types (swc fast strip), keeping every line and column; `Host::typescript` runs it first |
| `bindings/python/python/drawland/__init__.py`, `bindings/node/index.js` | The Python and Node faces: `Graph` with the namespaces |
| `crates/drawland-script/drawland.d.ts` | Type reference for editor + docs |
| `configs/*.js` | Example profiles |
| `crates/drawland/postcard.js` | The built-in default (`drawland::POSTCARD`) |

## Execution model

1. `library.js` is evaluated to a function, called with `dl` (natives: `call`, `prop`, `get`, `outputBegin`, `remapGrid`, `viewBegin`, `burin`…); its namespaces become the script's frozen globals.
2. Profile source runs; each call goes to `Graph::call` with its arguments encoded (handles marked `{ "$area": … }`), at the user's line (`Graph::at`, from the JS stack), and appends defs / draws / compose entries.
3. `evaluate` finishes the graph (`Graph::finish` validates) and returns `Grammar`. The interpreter is discarded before render.

What takes a function is orchestrated by each face in its own language, with the graph's primitives, so the library never calls back into a language: `view(spec, fn)` is `view_begin` / the function / `view_end`; a `remap` warp is `output_begin` (the output without remaps; false in a draft), then per remap `output_warps(done)`, `remap_grid` (the page points), the function on each, `remap_mesh`; `burin.terrain((b) => …)` runs burin's builder in Boa (`dl.burin`) and passes the atlas JSON, which Python, Node and Rust pass directly. A view in a style by name runs that JS style into the same graph (`run_into`), from any face.

Limits: `LOOP_LIMIT`, `RECURSION_LIMIT` in `lib.rs`. Errors format as `line N column M: …` using `SCRIPT_PATH` (`profile.js`).

## API groups (top-level)

`output`, `title`, `features`, `areas`, `lines`, `fills`, `engine`, `page`, `world`, `burin`, `ink`, `modules`, `draw`, `paint` (a screen's layer, see [screens.md](screens.md)), `compose`, `view`: every function in `drawland_graph::FUNCTIONS`, every value in `PROPERTIES`; `drawland.d.ts` declares them (a test compares it with the JS face's `library_names()`).

## The frame: `page` and `world`

What the script can ask while it runs, like a web page asks `window`: the
frame, never the data (see [decisions.md](decisions.md#scripts-read-the-frame-never-the-data)).

- `page.width`, `page.height`, `page.center` (mm), `page.frame` (the paper as an `Area`), `page.toWorld([x, y])` → `{ lat, lng }`.
- `world.scale`, `world.bearing`, `world.projection` (from the output alone), `world.center`, `world.bounds` (`{ north, south, east, west }` round the page's edge), `world.toPage({ lat, lng })` → `[x, y]`.
- `world.zoom` (Mapbox's zoom at the page's middle, `drawland_core::expr::Frame`: a unit a millimetre on paper, a pixel or a character on a screen) and `world.at(expression)`: a Mapbox expression's value here (`world.at(["step", ["zoom"], 1, 15, 3])`).

**Ranges by zoom or scale.** A Mapbox expression that reads `["zoom"]` or `["scale"]`, written anywhere in a call's options (`modules.roads({ classes: { residential: ["step", ["zoom"], ["literal", { singleLine: true }], 14, ["literal", { ground: 2.5 }]] } })`), is worked out before the call, at the frame (`Library::call` → `at_frame`), so every option of every function takes one without its own code; `world.at` is the same, typed for a script (`at<T = any>`), since the d.ts types options as plain values. The graph is made for one frame (a sheet, a scene, a view), so the value is known when the script runs, and the graph holds the value, not the expression: a host that changes the scale runs the script again, as it does for a move. A screen's `paint` is the exception: its looks keep their expressions, worked out at every frame of a moving screen ([screens.md](screens.md#styling-by-zoom-and-feature)). An expression in options that reads a feature's tags is refused (tags are read per feature only by a screen's names; on paper, a module's classes and label rules do that).

`Graph::get_property` and `page.toWorld` / `world.toPage` answer from the innermost builder's output (or a view's keys when its style never called `output`), through `drawland_core::frame::Frame` (`frame.rs`): the output's projection, bearing and warp — the same `to_page` that places `at: { lat, lng }` in the engine. The frame is worked out once per output and cached on the builder. A `remap` warp (`warp: { remap: ([x, y]) => [x, y], grid }`) is a function, sampled as above before the script goes on: the warps before it are set (`output_warps`, which drops the cached frame) and the function runs on a `(grid + 1)²` lattice over the page, so `page.toWorld` inside it answers through the earlier warps. The samples go to the engine as a `mesh` warp; the graph never holds a function.

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
- `view({ id, at, size, lat, lng, scale }, "city-grain" | () => {…})` — another place drawn into a rectangle of the page. The graph keeps a stack of builders: a function style builds the view's graph in place; a name asks the host for that style's script (`Host::styles`, a `Styles` closure: the CLI reads `name.js` beside the profile, the web page lends its presets; no host styles is an error) and runs it in a fresh interpreter. Inside a view, `output()` keeps the view's place and size over the style's own, so `page` and `world` are the view's.

## Authoring conventions

For substantial profiles (`configs/*.js`), follow `.cursor/rules/profile-style.mdc`:
- Group all visual constants, stroke specs, buffer widths, fill patterns, and taxonomy tables at the top (`1. PARAMETERS & STYLE PALETTE`).
- Execute feature extraction/geometry (`2`), `draw` declarations (`3`), `compose.mask` / `compose.order` (`4`), and plotter optimization (`5`) linearly below without inline magic numbers.
- One stroke width per pen; weight comes from offset double lines, never a wider stroke (`ink.width` is for print/screen styles such as the Schoolroom atlas).

## When changing the API

1. `drawland-graph`: the function in `library.rs` (and `FUNCTIONS`), its typed method in `api.rs`
2. `drawland.d.ts` (and tests that keep d.ts aligned); the JS, Python and Node faces need nothing unless the call takes a function
3. Subsystem docs here if behaviour or contracts change
4. The web page's reference and Concepts (its own repository) are regenerated from `drawland.d.ts` there
