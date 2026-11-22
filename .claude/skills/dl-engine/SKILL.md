---
name: dl-engine
description: Finish an accepted draw.land change in the engine repository — the library once in library.rs, types in drawland.d.ts, tests, example profiles type-checked, architecture docs and README updated, committed on the working branch and fast-forwarded to main. Use when the user accepts an MVP, says "make it proper", "finish it", "put it in the engine", or "in main".
---

# Ship level 2: the engine

Everything in this repository, finished. Includes `dl-mvp`'s result; adds what makes it permanent.

## Do

1. **One library.** A new function or option goes in `crates/drawland-graph/src/library.rs` once (plus `FUNCTIONS` / `PROPERTIES` if it is a new name), with its typed method in `api.rs` (the Rust face). JavaScript, Python and Node read the library by name.
2. **Types.** Declare it in `crates/drawland-script/drawland.d.ts`, with a doc comment a user can learn from (an example call). `tests::the_description_declares_exactly_what_the_library_defines` fails if the two disagree.
3. **Tests.** A focused test where the logic lives (core unit test, or `crates/drawland-script/src/tests.rs` for a script-level behaviour). `cargo test` all green.
4. **Profiles type-check.** Every touched profile in `configs/`:
   `bindings/node/node_modules/.bin/tsc --noEmit --allowJs --checkJs --strict --target es2022 --lib es5 crates/drawland-script/drawland.d.ts configs/X.js`
   Widened constants get a `/** @type */`.
5. **Docs** (see "Where a feature is documented" below): the subsystem file in `docs/architecture/`, `INDEX.md` if files moved, `decisions.md` if the why matters, `README.md` if users meet it.
6. **Commit** on the working branch with the attribution line, then `git checkout main && git merge --ff-only <branch> && git checkout <branch>`. Leave the user's own uncommitted files out (stash them around the merge if needed). No push.
7. Rebuild the release binary and restart any `drawland serve` the user is watching.

## Where a feature is documented (engine side)

| What | Where |
|---|---|
| Every function, option, property | `drawland.d.ts` doc comments (the online reference is generated from it) |
| How it works, invariants | `docs/architecture/<subsystem>.md` |
| Why this way | `docs/architecture/decisions.md` |
| Users' first look | `README.md` |
| Example | a profile in `configs/` |

## Done when

Tests green, profiles type-check, docs updated, `main` fast-forwarded. Say which docs changed. Offer `dl-faces` and `dl-site`.
