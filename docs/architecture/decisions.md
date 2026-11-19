# Architectural decisions

Short rationale for choices that are easy to misread from code alone. Add entries when the team makes a durable change; link to subsystems for detail.

## Profile builds a graph, engine runs it

JavaScript never holds geometry. A host can load the library and the engine separately, and can run a profile without our interpreter: the web page runs it in the browser's own JavaScript, over `library.js` and `drawland-graph` compiled to WASM, and loads that only when a script runs. The CLI can dump and load JSON graphs.

## The library is Rust; every language is a face

`drawland-graph` holds every function a style calls, with its checks, defaults and messages. JavaScript (`library.js`, a thin function of the library's calls), Python, Node and Rust are faces of it, so none can drift and none is second-class: a style can be written in the language of the program it lives in, and the graph is the same. JavaScript/TypeScript stays the form of a shareable style (it runs sandboxed, on the web page too). A face orchestrates only what takes a function in its own language (`view`, `remap`), with the graph's primitives, so the library never calls back into a language.

## TypeScript by blanking the types

A `.ts` profile has its types replaced by blanks (`drawland-ts`, swc's fast strip), not transpiled: every line and column of the JavaScript is the TypeScript's, so errors point where the author wrote. What would need code (`enum`, `namespace`) is an error. On the web page the stripper is a WASM module of its own, loaded only for TypeScript.

## Vector tiles default; Overpass optional

Tiles are fast CDN fetches with OpenMapTiles classes; Overpass retained for tags tiles lack and for comparison presets. Tile logic centralized in terramask + `tiles.rs`.

## Sibling libraries by path

penfill, terramask, burin-map evolve with draw.land; registry versions are not the source of truth in this monorepo layout. Policy: `.cursor/rules/our-libraries.mdc`.

## Compose masks target group ids

Occlusion is per draw id list, not automatic painter's order only. Presets can punch holes (roads through parks) without global subtraction.

## Projection modes

`webmercator` is a crop of a web map at the same place; `utm` is for survey-true local plots. Any other projection or warp is reached through `Frame` (`to_world` per page point), so a host can bend its own preview to match rather than learn each projection. Custom PROJ strings supported where proj4rs can compile without C (WASM).

## Deterministic profiles

No randomness, time, or network in the sandbox — same script + same inputs → same SVG.

## Scripts read the frame, never the data

A profile may ask about its **frame** while it runs — `page` (the paper) and `world` (the ground under it: centre, scale, bearing, projection, bounds, and `toPage` / `toWorld`) — because all of it follows from the output alone, before anything is downloaded. It may not read the map's **data** (elevations, features, addresses): those arrive after the script is gone, so anything that depends on them stays an engine operation that returns a handle (`areas.band`, `modules.labels`, `features.where`). The script text stays place-free and its graph depends on the place, as a web page's layout depends on `window`: the web app already recompiles on every move of the card, and the CLI passes its `--lat/--lng` in. Before, the script could not know where it was, and every ground-tied idea needed its own engine module (the graticule). Data a script wants to style by goes the other way: the engine turns it into features whose values are tags (`features.contours`, `features.soundings`: `ele`, `depth`, and `t` across the map's own range), and the script filters, colours and labels them with `features.where`, `ink.ramp` and `modules.labels` without reading a number.

## A screen is painted, not drawn

A screen (`output.screen`: a 1-bit bitmap or text cells) is covered by `paint` entries, a list of its own beside `draw`. A plotter's strokes (hatches, offset double lines, single-stroke letters) are made for a pen on paper; on a grid of pixels or cells they alias, moiré and become unreadable. So a screen paints the graph's areas, lines and named features with looks made for a grid (patterns, tones, tiled characters, Braille, box drawing, pixel fonts), and a style can carry both lists. The evaluation is the engine's own (`grammar_run::paint`); the grid is a crate of its own (`drawland-grid`), so the plotter core stays the plotter's. The output's unit is the screen's own: a pixel, or a character (one across, two down, so shapes stay square). A profile for a screen counts what it paints, not millimetres of a paper it will never have, and the scale is the ground a unit spans (`"3m"`). Inside, the scale already was the ground a page unit spans, so `Output`, `Frame` and every op read it unchanged. Patterns are read in world coordinates, so a map moved under them keeps them on the ground. A screen that moves is painted from a **scene**, the layers evaluated once for an area wider than the screen: a frame is only rasterising and placing names again (milliseconds), so a map can follow the pointer the way MapSCII does; the graph runs again only when the view nears the scene's edge or zooms half a step away.

## Core has no I/O

All HTTP and filesystem live in the hosts (the CLI here, the web page's workers in its own repository); the core receives bytes. Simplifies testing and parity between front ends.

## Patterns are penfill's catalogue, described once

penfill owns every pattern: its name, its options (kind, default, doc) and their checks. draw.land translates option values and nothing else; its d.ts pattern block, the docs' Patterns page and the swatches are generated from penfill's `describe()`. Before, draw.land kept a list of names, a per-name spacing correction table and its own parameter plumbing, and they drifted from penfill. The memory budget follows the same rule: it samples the real fill instead of keeping per-name coefficients.
