---
name: ship-faces
description: Carry a draw.land engine change to every face of the library — Python, Node, Rust crates — building and testing each and updating their READMEs, so all faces agree. Use after ship-engine when the user says "all wrappers", "bindings", "Python and Node too", "everywhere", or before a release.
---

# Ship level 3: engine and all its faces

Run `ship-engine` first if it is not done. Then make sure Python, Node and Rust say and do the same.

## Do

1. **Node** (`bindings/node`): `npm run build && npm test`. The build regenerates `bindings/node/library.d.ts` from `drawland.d.ts`: commit it.
2. **Python** (`bindings/python`): needs a virtualenv. If none is active, make a throwaway one in the scratchpad:
   ```bash
   python3 -m venv $SCRATCH/venv && $SCRATCH/venv/bin/pip -q install maturin pytest
   cd bindings/python && VIRTUAL_ENV=$SCRATCH/venv PATH=$SCRATCH/venv/bin:$PATH maturin develop && VIRTUAL_ENV=$SCRATCH/venv PATH=$SCRATCH/venv/bin:$PATH pytest -q
   ```
3. **The faces agree**: `cargo test -p drawland-script --test faces`, the Python and Node tests compare their graphs with JavaScript's.
4. **Try the new thing from each face** in two lines (Python `g.world.zoom`, Node `g.world.at([...])`, Rust `g.world().zoom()`): a README must not promise what a face cannot do.
5. **READMEs** where users of each face read: `bindings/python/README.md`, `bindings/node/README.md`, `crates/drawland-graph/README.md`, `crates/drawland/README.md` (and `crates/drawland-ts/README.md` for TypeScript).
6. Commit and fast-forward `main` as in `ship-engine`.

## Don't

Publish anything: that is `ship-release`.

## Done when

All three faces build, their tests pass, each was tried, READMEs say it. Offer `ship-site` and `ship-release`.
