# Pipeline overview

## Purpose

Turn a **place** and a **profile** into a single layered **SVG** measured in paper millimetres. Post-SVG tooling (G-code, optimisation) is out of scope.

## Stages

1. **Compile profile** — JavaScript runs in `drawland-script` (Boa). It only constructs a `Grammar`; no geometry is computed in JS.
2. **Resolve place** — Centre `(lon, lat)` from CLI flags, geocoding, or `output({ lat, lng, … })` in the profile.
3. **Download** — Whatever the graph references:
   - Overpass JSON when any def uses `osm_select` (`Job.needsOsm` / CLI equivalent).
   - Vector tiles for `tiles_select` (OpenFreeMap / terramask inside `tiles.rs`).
   - Terrarium terrain PNGs when burin terrain or `tools_terrain` is used.
   - Google font bytes when labels request `{ origin: "google", … }`.
4. **Run** — `grammar_run::run` evaluates the def DAG, then applies `draw` + `compose` (masks, clip) and writes SVG.

A screen output (`output.screen`) ends differently: `grammar_run::paint` hands each `paint` entry its geometry and `drawland-grid` paints a bitmap or text cells (see [screens.md](screens.md)).

## Output model

`Output` (`output.rs`) holds centre, `width_mm` / `height_mm`, scale denominator, bearing, projection mode, optional `tile_zoom`. Ground extent follows from scale and paper size; `query_polygon()` drives Overpass and tile selection: the page's edge taken back through the projection (`Projector::unproject`) for web Mercator and PROJ, a geodesic rectangle walked from the centre for UTM. A walked rectangle is wrong far out: a web Mercator scene a continent wide reaches much further north than a geodesic walked east from the top's middle, which bends back toward the equator and cut the north off the tiles read.

## Projection

All features end in **output mm**, **y down** (`projection.rs`):

- `utm` — UTM zone of centre (true distances; meridian convergence away from centre).
- `webmercator` — EPSG:3857 with scale tied to centre latitude (matches web map preview).
- Custom PROJ string via proj4rs (no default C deps; WASM-safe).
- `+proj=ortho` (a **globe**, which proj4rs lacks) is drawland's own (`ortho.rs`), on web Mercator's sphere: `Projector::globe`. Half the Earth is out of sight, so every geometry is cut at the horizon in lon/lat before it is projected (`Projector::visible`, in `grammar_run`'s projection and `Engine::warp_project`): on the unit sphere turned so the view's centre is its top, long edges first split to a degree; a line is cut where it crosses the horizon; an area's rings are turned by role (outside counter-clockwise, holes clockwise, whatever the data drew) and cut by Weiler–Atherton: each run in sight, then the horizon the ring's way round to where the next run comes back. A ring that leaves and comes back at one point (an area as wide as the world, down one side of the antimeridian and up the other) closes with the whole rim; a ring wholly out of sight is the whole disc when it holds the view's centre. Points in sight within half a degree of the rim are pulled in to it (≤ 240 m): the rim is drawn as one-degree chords, and a shore outside a chord would cross it, and a ring that crosses itself is nothing to a union. A point out of sight is left unprojectable (NaN), and `labels::named` names no such pin. Its query area is the cap the page's edge reaches (the near side when the edge is off the globe; `Ortho::ground`), and its tiles are joined as on any page: the pole's `pieces` and the self-union (`mended`) are for areas that meet themselves along the antimeridian once projected, which a globe's 3D cut never makes.

**A pole on the page** (a PROJ projection that reaches it, e.g. `+proj=stere +lat_0=90 +lon_0={lng}`; the output's place need not be the pole, only on a page that holds it): `Output::query_polygon` is the cap from the edge's farthest latitude to the pole, every longitude; `Tiles` clamps its box a hair inside web Mercator's edge (±85.05°, ±180°: a clip box on the tiles' own edges, or at the pole, hangs i_overlay's OGC extraction) and keeps each tile's area pieces apart (`pieces`: joined across every longitude, an area meets itself along the antimeridian once projected); each projected area is its own union (`mended` in `grammar_run.rs`); and round the North Pole the cap above 85° is added as one more ocean feature (`arctic_cap`), the South Pole's left as land. `Frame::bounds` then reports the pole and every longitude.

After projecting, `output.warp` (`warp.rs`) bends every geometry in page mm: `fisheye`, `twirl` and `wave` keep the page's edge; a `mesh` (a grid of where page nodes go, Catmull–Rom between them, the edge node's move off the grid) may move it. Then the engine's frame and drawable area are the bent page clipped back to the paper (`grammar_run.rs`), so the page's ground is drawn where the mesh puts it; outside that frame the mesh is only carried on, so the paper there masks every drawn group (`beyond` in `compose`). Data is still fetched for the projection's page, before the warp. A mesh is how a script writes a projection of its own (see [profiles.md](profiles.md)).

`Frame` (`frame.rs`) is an output's page and ground together: `to_page(lon, lat)` (projection, bearing, then warp — where the drawn map puts a place, as `at: { lat, lng }` uses it), `to_world(x, y)` (Newton steps undoing the warp in page space from the point itself, then the projection's own `locate`) and `bounds()` (the ground round the page's edge). It is what a script's `page` / `world` read, while it runs, from its `output` alone (see [profiles.md](profiles.md#the-frame-page-and-world)).

## Typed graph sections

Features → areas (`MultiPolygon`) / lines (`geom::Lines`) / fills (lines from penfill). Modules (roads, labels) publish multiple named outputs from one def. See [grammar-engine.md](grammar-engine.md).

## Front ends

| Front end | Compile profile | Download | Run |
|-----------|-----------------|----------|-----|
| CLI, Python, Node | `drawland_script::evaluate_in` | `drawland::Drawer` + terramask fetch | `grammar_run` in-process |

The draw.land web page is another front end, in its own repository: it compiles the same crates to WASM and hands the engine the bytes its workers fetch.

## Key invariants

- Profiles must be deterministic: sandbox strips `Date`, `Math.random`, network, and filesystem from JS.
- Geometry is computed once per def in the DAG; downstream defs read cached values.
- SVG group ids come from `draw({ id: … })`; compose masks reference those ids.
