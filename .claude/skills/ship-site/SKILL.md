---
name: ship-site
description: Carry a draw.land engine change to the draw.land web page (../drawland-web) — WASM rebuilt, presets re-run, online docs and reference updated, doc pictures drawn, the gallery styles using it where they should, checked in the browser — committed there, not deployed. Use when the user says "website", "online docs", "on the site", "in the presets/styles", "in City Grain", or wants to see it on the page.
---

# Ship level 4: the website

The engine change must be done (`ship-engine`). The page is the private repository `../drawland-web` (read its `AGENTS.md` and `docs/architecture/web.md`); it builds on `../drawland` directly.

## Do (from `../drawland-web`)

1. **WASM**: `./web/build-wasm.sh`.
2. **Reference**: `node web/scripts/build-reference.mjs` (regenerates `web/src/docs/library-reference.json` from `drawland.d.ts`).
3. **Presets**: `cargo run -q --example check_presets` → if only STALE (script positions moved), `-- --write`. A FAIL is a real error.
4. **Use it in the styles** where it helps (`web/src/presets/*.js`). A preset must draw its own scale exactly as before unless the user asked for a new look: compare `drawland map preset.js --dump-graph` before and after (strip `at`), and render a changed scale to show the gain.
5. **Online docs** (`web/src/docs/`), wherever a user would look for it:
   - For developers (`Developers.jsx`): engine, faces, screens, the graph.
   - Styles, simplified (`examples/lite-*.js` + `lite.json`, `Lite.jsx`): a short real profile that teaches the idea; `scales` in `lite.json` draws it at more than one scale.
   - Tutorial (`examples/tutorial-*.js`), Concepts (`Concepts.jsx`) when the idea is basic.
   - Every example is a real profile type-checked by `npm test`; inline `<Code>` snippets are not, so keep them small and correct.
6. **Pictures**: `./web/scripts/render-docs.sh examples <prefix>` (or `gallery <preset>`), needs `rsvg-convert` and `magick`. Commit the `.webp` files.
7. **Tests**: `(cd web && npm test)`.
8. **Look at it**: start the page with the `serve-local` skill and check the changed sections (and a phone width when layout changed: Chrome DevTools-protocol emulation, 390×844 @3x, touch on; plain headless Chrome cannot go under 500 px).
9. **Docs of the page**: `docs/architecture/web.md` (and `INDEX.md` if files moved).
10. Commit in `../drawland-web`. Do not run `web/deploy.sh` unless the user asks: deploying publishes draw.land (`ship-release`).

## Done when

Presets 17/17 ok, `npm test` green, pictures drawn, page checked in a browser, committed. Tell the user the local URL.
