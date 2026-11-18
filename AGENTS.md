# Agent instructions (draw.land)

draw.land turns OpenStreetMap data into layered pen-plotter SVGs. A **style** (a JavaScript or TypeScript **profile**, or calls from Python, Node or Rust) builds an operation **graph**; **drawland-core** downloads map data and runs the graph. The `drawland` CLI is the host in this repository (it does the I/O; the core does none). The draw.land web page is another host, closed source, in its own repository: it builds on these crates, so keep their public API (`drawland_graph::Graph`, `drawland_script::evaluate_in`, `Host`, `drawland_core`) deliberate. The library is Rust (`drawland-graph`); JavaScript/TypeScript, Python, Node and Rust are equal faces of it: a new function goes in `library.rs` once.

## Knowledge base (read selectively)

1. Start at [`docs/architecture/INDEX.md`](docs/architecture/INDEX.md) — subsystem map and links.
2. Open **only** the subsystem doc(s) that match your task.
3. Verify behavior in **source** when docs might be stale. Code wins over docs.

| Quick pointer | Where |
|---------------|--------|
| The library (every function a style calls) | `crates/drawland-graph/src/library.rs` (+ typed `api.rs`) |
| Profile API (exact types) | `crates/drawland-script/drawland.d.ts`; JS face `crates/drawland-script/src/library.js` |
| Graph schema & runner | `crates/drawland-core/src/grammar.rs`, `grammar_run.rs` |
| User-facing overview | `README.md` |
| Sibling libs (penfill, terramask, burin-map) | `.cursor/rules/our-libraries.mdc` |

Do **not** load every architecture file for small edits. Do **not** treat `reference/python/` as runnable product code.

## Repo layout (crates)

| Crate | Role |
|-------|------|
| `drawland-graph` | The library: builds the graph from calls, by name or typed |
| `drawland-core` | Graph validation, geometry, fills, compose, SVG — no network/files |
| `drawland-script` | Boa interpreter + `library.js` (the JS face) → graph |
| `drawland-ts` | TypeScript profiles: types blanked, positions kept |
| `drawland` | The host as a library: geocode, Overpass, tiles, terrain, fonts, cache → `Drawer::draw`; `cli::run` is the command line |
| `drawland-cli` | The `drawland` binary (`main` over `drawland::cli::run`) |
| `bindings/python`, `bindings/node` | `pip install drawland`, `npm install drawland`: thin wrappers over the `drawland` crate |

Example profiles: `configs/`; the built-in default: `crates/drawland/postcard.js`. The web page's presets are not here.

## Maintenance (substantial tasks)

When you change responsibilities, interfaces, data flow, or invariants:

1. Update the relevant `docs/architecture/<subsystem>.md` (and `decisions.md` if the rationale matters).
2. Update `docs/architecture/INDEX.md` if files were added, removed, or renamed.
3. In your final message, say which docs changed or that none were needed.

Skip doc updates for trivial fixes. Follow `.cursor/rules/knowledge-base.mdc`.

## Working style: MVP first

Try new ideas end to end on the **CLI first**: change the Rust engine (penfill
and/or `drawland-core`), write or tweak a profile in `configs/`, and render it
with `drawland map <profile>` (or `./generate.sh`) to look at the SVG. Only
once the result looks right do we carry it to the web page (its own
repository, `../drawland-web`) and penfill's Python / Node / wasm bindings. Don't widen a
change to every surface in the same step unless asked.

## Build & test

```bash
cargo test
(cd bindings/node && npm run build && npm test)
(cd bindings/python && maturin develop && pytest)
```

The faces must agree: `drawland-graph/tests/api.rs`, the Python and Node tests compare a graph built in each with the same profile in JavaScript.

Releases: push, let `.github/workflows/build.yml` build the wheels and addons, then `scripts/publish.sh <run-id>` (crates.io, PyPI, npm).

After changing a sibling library (`../penfill`, `../terramask`, `../burin-map`, `../deoverlap`), build and test there first, then rebuild draw.land.
