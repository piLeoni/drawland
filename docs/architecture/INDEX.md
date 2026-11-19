# Architecture index

Compact map of the draw.land engine and its command line (the web page lives in its own repository). Read this first, then open **one or two** subsystem docs — not the whole folder.

## End-to-end flow

```
Profile (.js/.ts) → drawland-script (Boa) ┐
Python / Node / Rust Graph ───────────────┴→ drawland-graph → Grammar (JSON)
       →  fetch OSM / vector tiles / terrain / fonts (CLI or web worker)
       →  grammar_run  →  layered SVG (mm)
                       →  paint entries → drawland-grid → bitmap / text screen
```

See [pipeline.md](pipeline.md).

## Subsystems

| Subsystem | Doc | Primary locations |
|-----------|-----|-------------------|
| Pipeline & units | [pipeline.md](pipeline.md) | `output.rs`, `projection.rs`, `frame.rs`, `warp.rs`, `units.rs`, `profile.rs` |
| Graph & execution | [grammar-engine.md](grammar-engine.md) | `grammar.rs`, `grammar_run.rs`, `geom.rs`, `optimize.rs` |
| The library and its faces | [profiles.md](profiles.md) | `drawland-graph/` (the library), `drawland-script/` (JS face, `drawland.d.ts`), `drawland-ts/`, `bindings/`, `configs/` |
| Map data, host & bindings | [cli-data.md](cli-data.md) | `crates/drawland/` (library + CLI), `drawland-cli/`, `bindings/python`, `bindings/node`, `.github/workflows/build.yml`, `scripts/`, `osm.rs`, `tiles.rs`, `osm_cache.rs`, `geojson.rs` |
| Fills & strokes | [fills-compose.md](fills-compose.md) | `fill.rs`, `budget.rs`, `optimize.rs`, `profile.rs`, `svg.rs`, penfill catalogue, `drawland-script/src/catalog_dts.rs` |
| Isoline blob fills (design) | [isoline-blob-fills.md](isoline-blob-fills.md) | penfill (new generator), `fill.rs`, compose masks |
| Terrain (burin) | [terrain-burin.md](terrain-burin.md) | `terrain.rs`, `contours.rs`, `burin.*` in profiles |
| Screens (bitmap, text) | [screens.md](screens.md) | `paint.rs`, `grammar_run::paint`, `drawland-grid/`, `drawland/src/view.rs`, `drawland/src/serve.rs` (+ `serve.html`), `configs/terminal.js`, `configs/e-ink.js` |
| Labels & text | [labels.md](labels.md) | `labels.rs`, `text.rs`, `stroke_font.rs`, `google_fonts.rs`, `graticule.rs` |
| Decisions | [decisions.md](decisions.md) | Rationale that spans crates |

## External libraries (sibling repos)

penfill, terramask, burin-map — path dependencies, change in-repo when needed. Summary: `.cursor/rules/our-libraries.mdc` (not duplicated here).

## Other documentation

| Topic | Path |
|-------|------|
| Product README | [../../README.md](../../README.md) |
| Legacy Python pipeline (reference only) | `reference/python/docs/` |

The API's source of truth is `crates/drawland-script/drawland.d.ts`; the reference at draw.land/docs is generated from it.
