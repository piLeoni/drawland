# Screens: bitmaps and text

## Purpose

The same graph, painted on a grid instead of drawn on paper: a 1-bit **bitmap** (every pixel black or white; PNG or PBM) or **text** cells (a terminal; plain text, ANSI truecolour, HTML, or a PNG picture of the cells). `output({ screen })` makes the output a screen, and `paint(...)` entries cover it, bottom first. A plotter's `draw` entries and `compose` (masks, optimize) are not used on a screen: a layer's `halo` is how it clears what lies under it.

## Key files

| File | Role |
|------|------|
| `drawland-core/src/paint.rs` | The spec: `Screen` (`kind`, `width`, `height`, `invert`; `cell`, `dot`, `size` in the output's units), `CELL_DOTS`, `Paint` and its `Look` (`FillPaint`, `LinePaint`, `TextPaint`), `Halo`, `Tile`, `Marker`, `Rgb`, the pattern and font names |
| `drawland-core/src/profile.rs` | `OutputSpec::screen`, `OutputSpec::fit_screen` (the output's size in the screen's units) |
| `drawland-core/src/grammar.rs` | `Grammar::paint`, its JSON (`paint: [{ id, from, fill | line | text, halo }]`), `validate_paint` |
| `drawland-core/src/grammar_run.rs` | `grammar_run::paint` → `Vec<Painted>`: the graph evaluated as usual (`Want::Paint`), each entry handed its area, lines or names |
| `drawland-core/src/labels.rs` | `labels::named`: the names of features before any font (text, rank, road chains), shared with the pen's labels |
| `drawland-grid/` | The painter: `scene.rs` (`Scene`, `View`: a screen that moves, painted from geometry evaluated once), `raster.rs` (scanline areas, Bresenham lines, cell walks, halo growth), `pattern.rs` (8×8 patterns, Bayer tone), `font.rs` (Spleen PSF fonts, built in), `place.rs` (names on the grid), `bitmap.rs`, `text.rs`, `lib.rs` (`render`, `origin`, `Picture`, `Format`) |
| `drawland/src/lib.rs` | `Drawer::paint` / `paint_graph`, `Drawer::make` (paper or screen, as the output says), `Painting::save` |
| `drawland/src/view.rs` | `drawland view`: the map in the terminal, moved with the keys; `open` (a screen profile and its place), shared with `serve` |
| `drawland/src/serve.rs`, `serve.html` | `drawland serve`: the map in a web page on this machine |

## Units

On a screen the output's unit is the screen's own, not the millimetre: a **pixel** on a bitmap, a **character** on text. Every length a profile writes (a buffer, an offset, `page.width`, a shape's points) counts them, and `scale` is the ground one of them spans: `scale: "3m"` is three metres a pixel (or a character). Inside, the scale stays what it is on paper, the ground a unit spans in mm (`"1:10000"` is a millimetre spanning 10 m, `"3m"` a unit spanning 3 m, both `Scale(10000.0)`/`Scale(3000.0)`), so `Output`, `Frame` and every op read it unchanged; `world.scale` is that number. A character is one unit across and two down (a row is as tall as two characters are wide), so geometry stays square and a circle stays round on the screen; a dot (`CELL_DOTS`, 2 × 4 a character, a Braille character's grid) is half a unit. `fit_screen` writes the size (`width` pixels or columns, `height` pixels or twice the rows) into `width` and `height`; `output()` refuses `width`, `height`, `padding` and `slices` beside a `screen`. A text screen is painted on its dot grid (`2w × 4h`) where it needs sub-cell detail (Braille, quarter blocks) and on its cells otherwise. A length written with a unit (`"2mm"`) still reads as that many mm and so as that many units; write bare numbers on a screen.

## Painting

`drawland_grid::render(screen, origin, layers)`:

1. Names are placed first (`place.rs`), the top text layer first, each layer's in `labels::named` order: whole pixels or cells, inside the grid, none within `padding` of another, `spacing` between two of one name. A point is centred, or beside its `marker` (right, left, above, below). An area's name goes at its interior point when the box fits inside it. A road's goes level on a straight, level stretch, or one letter under the other on a stretch running down the screen (a bitmap turns the letters a quarter); with neither, level across the road if the road is as long as the name.
2. Each layer in order: its ink's cells or pixels grown by `halo` are cleared out of what is under it (a bitmap lays the other ink there; text blanks the characters and keeps the background colour), then it is painted:
   - **fill**: `pattern` (8 × 8, `pattern.rs`) or `tone` (ordered Bayer dither); on text the same in Braille dots; `chars` tiled per row; `blocks` (quarter blocks from the dots); `background` (text cells' colour). `opaque` (default) clears the pattern's gaps.
   - **line**: a bitmap line is stepped pixel to pixel (no anti-aliasing), widened by a round brush, dashed by steps. On text: Braille dots (default); box drawing (`box`, `heavy`, `double`, `round`) by cell walks, each cell keeping the sides its line leaves by, so lines that meet join (`┼`), and a stretch slanted more than 20° falls back to Braille rather than a staircase; `ascii` by slope (`- | / \`, `+` where they meet); `char`.
   - **text**: Spleen glyphs on a bitmap, a letter a cell on text; marks.
3. Braille dots, box joins and ASCII strokes in one cell add up across layers of the same colour and style.

Patterns, tones and tiled characters are read at the dot's place in the **world**, not on the screen: `origin(output, screen)` is the screen's top-left in web Mercator dots at the output's scale, so a moved map keeps its waves on the water. Other projections read them from the screen's corner.

## Hosts

- `drawland map <profile>` writes a screen by the extension of `-o` (`.png`/`.pbm` bitmap, `.txt`/`.ans`/`.html` text, `.png` a picture of text cells; a bitmap written as text is drawn in Braille); `-o -` prints it to the terminal. Unset: `map.png` or `map.ans`.
- `drawland view <profile>` (unix): raw mode on the alternate screen, the screen sized to the terminal (a bitmap in Braille, 2 × 4 pixels a cell); arrows / `hjkl` move a quarter screen (`Frame::to_world`), `+`/`-` halve or double the scale, `r` repaints, `q` quits. Keys pressed while a paint runs are gathered into one move. It changes the graph's output between paints; the script is not run again, so a script that reads `world` sees the first place.
- `drawland serve <profile> [--port 8700]` (any platform): the map in a web page at `http://localhost:<port>/`, one profile a port, so a 1-bit and a text site run side by side as the separate sites they will be. It moves as MapSCII does: the graph is not run per move. `serve.rs` builds a **scene** (`Drawer::scene_graph` → `drawland_grid::Scene`): the `paint` layers evaluated once for a screen three times the view across and down, on the main thread, which owns the `Drawer`. Its vector tiles are as deep as the view's detail (`View::tile_zoom`: a 512-pixel tile's pixel no wider than a dot), as a web map picks them, so a scene is a few dozen tiles at any scale: far out it reads shallow tiles, not the thousand deepest the paper's cap would allow. Each frame (`/frame?lng&lat&scale&w&h`, one thread a kept-alive connection) is that scene's geometry moved and scaled to the view (`Scene::render`: pieces outside the view culled by their boxes, names placed again, patterns read at the view's own world origin), a few milliseconds; a bitmap frame is a PBM, a text frame JSON runs `[text, fg, bg]` a row. A frame moves the map by whole cells (pixels, characters), so names, lines and patterns keep their places on it from frame to frame, and says by `X-Shift` how far that leaves it off the view; the page draws it moved by that much. A view a quarter screen from the scene's edge, or half a zoom step from its scale (`Scene::covers`), asks for a new scene; frames go on from the old one meanwhile (`X-Scene: building`). The page (`serve.html`, built into the binary) draws on a canvas: every move first redraws the last frame shifted and scaled, on the map's ground colour, then asks for the next frame (one in flight, always for where the map is now). Drag, held arrows / `hjkl`, the wheel and a pinch (zoom about the pointer), `+`/`-` and the buttons (a short glide). Nothing on the page is dragged as an image or selected as text. A bitmap pixel is ×1–×8 **device** pixels (×2 by default on a retina screen); text has a font size. `size` is the window's or the profile's own `width × height`. The place and scale are kept in the address (`#lat,lng,scale`). Scenes are web Mercator, north up and unbent (`View::of`); `serve` refuses other outputs.
- Python and Node draw paper only for now (`Drawer::draw` refuses a screen); their faces have `paint`.
- The web page does not paint screens yet. `drawland-grid` does no I/O and builds for WASM as it is; `Text::cell(x, y)` gives a page each cell's character and colours.

## Profiles

`configs/terminal.js` (text, Venice), `configs/1-bit.js` (800 × 480 bitmap, Venice, a Nolli plan).
