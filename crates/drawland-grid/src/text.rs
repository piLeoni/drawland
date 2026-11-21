//! The text screen: a grid of cells, each a character in a colour over a
//! colour. Lines are Braille dots (two across, four down a cell), box
//! drawing joined where lines meet, or ASCII; areas are characters tiled
//! from a fixed point of the world, Braille patterns, quarter-cell blocks
//! or a background colour; names are written a letter a cell. Written as
//! plain text, ANSI (truecolour) or HTML.

use crate::pattern::{tone_at, Pattern};
use crate::place::{letter_at, Placed};
use crate::raster::{area_mask, cell_walk, clip, line_dots, Mask, ToDots};
use drawland_core::grammar_run::{PaintShape, Painted};
use drawland_core::paint::{FillPaint, LinePaint, LineStyle, Look, Marker, Rgb, CELL_DOTS};

/// A character in the output's units, across and down; a dot is half of one.
const CELL: (f64, f64) = (1.0, 2.0);
const DOT: f64 = 1.0 / CELL_DOTS.0 as f64;
use std::fmt::Write;

/// What a cell holds, before it is a character: Braille dots and box
/// joins add up as layers draw over them.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Blank,
    Char(char),
    /// The dots set, in Braille's own bit order.
    Braille(u8),
    /// The sides a box line leaves the cell by: north 1, east 2, south 4, west 8.
    Box(LineStyle, u8),
    /// The ASCII strokes through the cell: `-` 1, `|` 2, `/` 4, `\` 8.
    Ascii(u8),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    kind: Kind,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
}

impl Cell {
    const BLANK: Cell = Cell { kind: Kind::Blank, fg: None, bg: None };

    pub fn char(&self) -> char {
        match self.kind {
            Kind::Blank => ' ',
            Kind::Char(c) => c,
            Kind::Braille(0) => ' ',
            Kind::Braille(b) => char::from_u32(0x2800 + b as u32).unwrap_or(' '),
            Kind::Box(style, sides) => box_char(style, sides),
            Kind::Ascii(s) => match s {
                0 => ' ',
                1 => '-',
                2 => '|',
                4 => '/',
                8 => '\\',
                12 => 'X',
                _ => '+',
            },
        }
    }
}

/// A grid of cells, `width` columns by `height` rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub width: usize,
    pub height: usize,
    cells: Vec<Cell>,
}

/// Braille's bit for the dot `(x, y)` of a cell, x 0–1, y 0–3.
fn braille_bit(x: usize, y: usize) -> u8 {
    const BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    BITS[x][y]
}

/// The quarter blocks, by the quarters set: top-left 1, top-right 2,
/// bottom-left 4, bottom-right 8.
const QUARTERS: [char; 16] = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];

fn box_char(style: LineStyle, sides: u8) -> char {
    // By sides: N 1, E 2, S 4, W 8.
    const LIGHT: [char; 16] = ['─', '│', '─', '└', '│', '│', '┌', '├', '─', '┘', '─', '┴', '┐', '┤', '┬', '┼'];
    const HEAVY: [char; 16] = ['━', '┃', '━', '┗', '┃', '┃', '┏', '┣', '━', '┛', '━', '┻', '┓', '┫', '┳', '╋'];
    const DOUBLE: [char; 16] = ['═', '║', '═', '╚', '║', '║', '╔', '╠', '═', '╝', '═', '╩', '╗', '╣', '╦', '╬'];
    const ROUND: [char; 16] = ['─', '│', '─', '╰', '│', '│', '╭', '├', '─', '╯', '─', '┴', '╮', '┤', '┬', '┼'];
    let table = match style {
        LineStyle::Heavy => &HEAVY,
        LineStyle::Double => &DOUBLE,
        LineStyle::Round => &ROUND,
        _ => &LIGHT,
    };
    table[(sides & 15) as usize]
}

impl Text {
    pub fn new(width: usize, height: usize) -> Text {
        Text { width, height, cells: vec![Cell::BLANK; width * height] }
    }

    pub fn cell(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.width + x]
    }

    fn at(&mut self, x: i64, y: i64) -> Option<&mut Cell> {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return None;
        }
        Some(&mut self.cells[y as usize * self.width + x as usize])
    }

    /// These cells at the left of a blank screen `width` × `height`
    /// (cut where it is smaller).
    pub fn placed_in(&self, width: usize, height: usize) -> Text {
        let mut out = Text::new(width, height);
        for y in 0..self.height.min(height) {
            for x in 0..self.width.min(width) {
                out.cells[y * width + x] = self.cells[y * self.width + x].clone();
            }
        }
        out
    }

    /// The cell at `(x, y)` set.
    pub fn put(&mut self, x: usize, y: usize, cell: Cell) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = cell;
        }
    }

    /// The cell at `(x, y)` emptied: no character, no colour.
    pub fn clear(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = Cell::BLANK;
        }
    }

    /// An interface laid over the cells, in `palette`'s colours; where it
    /// has no cell the map shows.
    pub fn overlay(&mut self, chrome: &crate::ui::Chrome, palette: &crate::ui::Palette) {
        for y in 0..chrome.height.min(self.height) {
            for x in 0..chrome.width.min(self.width) {
                if let Some((ch, role)) = chrome.cell(x, y) {
                    let (fg, bg) = palette.at(role, y, chrome.height);
                    self.cells[y * self.width + x] = Cell { kind: Kind::Char(ch), fg: Some(fg), bg: Some(bg) };
                }
            }
        }
    }

    /// One Braille dot of the screen's dot grid set, in `fg`.
    pub fn dot(&mut self, x: i64, y: i64, fg: Option<Rgb>) {
        let (cx, cy) = (x.div_euclid(2), y.div_euclid(4));
        let bit = braille_bit(x.rem_euclid(2) as usize, y.rem_euclid(4) as usize);
        if let Some(cell) = self.at(cx, cy) {
            cell.kind = match cell.kind {
                Kind::Braille(b) if cell.fg == fg => Kind::Braille(b | bit),
                _ => Kind::Braille(bit),
            };
            cell.fg = fg;
        }
    }

    /// The characters, a line a row, the spaces at each line's end left off.
    pub fn txt(&self) -> String {
        let mut out = String::new();
        for y in 0..self.height {
            let line: String = (0..self.width).map(|x| self.cell(x, y).char()).collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    /// The characters in their colours, for a terminal: 24-bit SGR codes.
    pub fn ansi(&self) -> String {
        let mut out = String::new();
        for y in 0..self.height {
            let (mut fg, mut bg): (Option<Rgb>, Option<Rgb>) = (None, None);
            for x in 0..self.width {
                let c = self.cell(x, y);
                if c.fg != fg {
                    match c.fg {
                        Some(Rgb(r, g, b)) => {
                            let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
                        }
                        None => out.push_str("\x1b[39m"),
                    }
                    fg = c.fg;
                }
                if c.bg != bg {
                    match c.bg {
                        Some(Rgb(r, g, b)) => {
                            let _ = write!(out, "\x1b[48;2;{r};{g};{b}m");
                        }
                        None => out.push_str("\x1b[49m"),
                    }
                    bg = c.bg;
                }
                out.push(c.char());
            }
            out.push_str("\x1b[0m\n");
        }
        out
    }

    /// A picture of the screen, as a terminal would show it: each cell 8
    /// × 16 pixels in Spleen, Braille and quarter blocks drawn as dots and
    /// blocks. Unset colours are a terminal's, light grey on near black.
    pub fn png(&self) -> Result<Vec<u8>, String> {
        const FG: Rgb = Rgb(0xcc, 0xcc, 0xcc);
        const BG: Rgb = Rgb(0x0c, 0x0c, 0x0c);
        let f = crate::font::font("spleen-8x16").ok_or("the built-in font spleen-8x16")?;
        let (cw, ch) = (f.width, f.height);
        let (w, h) = (self.width * cw, self.height * ch);
        let mut rgb = vec![0u8; w * h * 3];
        let mut put = |x: usize, y: usize, c: Rgb| {
            let i = (y * w + x) * 3;
            rgb[i..i + 3].copy_from_slice(&[c.0, c.1, c.2]);
        };
        for cy in 0..self.height {
            for cx in 0..self.width {
                let cell = self.cell(cx, cy);
                let (fg, bg) = (cell.fg.unwrap_or(FG), cell.bg.unwrap_or(BG));
                let ink: Box<dyn Fn(usize, usize) -> bool> = match (cell.kind, cell.char()) {
                    (Kind::Braille(b), _) => Box::new(move |x, y| {
                        // Each dot a 2 × 2 square in its quarter of the column.
                        let (dx, dy) = (x / (cw / 2), y / (ch / 4));
                        let (ix, iy) = (x % (cw / 2), y % (ch / 4));
                        b & braille_bit(dx, dy) != 0 && (1..=2).contains(&ix) && (1..=2).contains(&iy)
                    }),
                    (_, c) if QUARTERS.contains(&c) => {
                        let q = QUARTERS.iter().position(|&k| k == c).unwrap_or(0);
                        Box::new(move |x, y| q & (1 << ((y >= ch / 2) as usize * 2 + (x >= cw / 2) as usize)) != 0)
                    }
                    (_, c) => Box::new(move |x, y| f.ink(c, x, y)),
                };
                for y in 0..ch {
                    for x in 0..cw {
                        put(cx * cw + x, cy * ch + y, if ink(x, y) { fg } else { bg });
                    }
                }
            }
        }
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().map_err(|e| format!("png: {e}"))?;
            writer.write_image_data(&rgb).map_err(|e| format!("png: {e}"))?;
        }
        Ok(out)
    }

    /// A page of its own: the cells in a `<pre>`, a run of one colour a
    /// `<span>`. Unset colours are a terminal's: light grey on near black.
    pub fn html(&self, title: &str) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "<!doctype html>\n<html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title>\n<style>\nbody {{ margin: 0; background: #0c0c0c; }}\npre {{ margin: 0; padding: 16px; color: #cccccc; background: #0c0c0c; font: 14px/1 ui-monospace, Menlo, \"DejaVu Sans Mono\", Consolas, monospace; letter-spacing: 0; }}\n</style></head><body><pre>",
            escape(title)
        );
        out.push_str(&self.html_rows());
        out.push_str("</pre></body></html>\n");
        out
    }

    /// The cells as the inside of an HTML `<pre>`: a line a row, each run of
    /// one colour a `<span>`.
    pub fn html_rows(&self) -> String {
        let mut out = String::new();
        for y in 0..self.height {
            let mut x = 0;
            while x < self.width {
                let c = *self.cell(x, y);
                let mut run = String::new();
                while x < self.width && self.cell(x, y).fg == c.fg && self.cell(x, y).bg == c.bg {
                    run.push(self.cell(x, y).char());
                    x += 1;
                }
                let mut style = String::new();
                if let Some(fg) = c.fg {
                    let _ = write!(style, "color:{};", fg.hex());
                }
                if let Some(bg) = c.bg {
                    let _ = write!(style, "background:{};", bg.hex());
                }
                if style.is_empty() {
                    out.push_str(&escape(&run));
                } else {
                    let _ = write!(out, "<span style=\"{style}\">{}</span>", escape(&run));
                }
            }
            out.push('\n');
        }
        out
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The layers painted on a grid of `w` × `h` cells whose top-left dot is
/// `origin` in the world's dots (where patterns and tiled characters are
/// read).
pub fn paint(layers: &[Painted], placed: &Placed, w: usize, h: usize, origin: (i64, i64)) -> Text {
    let (dx, dy) = (CELL_DOTS.0 as usize, CELL_DOTS.1 as usize);
    let dots = ToDots { per_dot: DOT };
    let mut text = Text::new(w, h);
    for (i, layer) in layers.iter().enumerate() {
        match (&layer.look, &layer.shape) {
            (Look::Fill(f), PaintShape::Area(a)) => {
                let mask = area_mask(a, dots, w * dx, h * dy);
                let touched = cells_of(&mask, w, h, 1);
                clear_halo(&mut text, &touched, layer);
                fill(&mut text, &mask, f, origin);
            }
            (Look::Line(l), PaintShape::Lines(lines)) => {
                let cells = line_cells(lines, l, w, h);
                clear_halo(&mut text, &cells, layer);
                line(&mut text, lines, l, w, h);
            }
            (Look::Text(t), PaintShape::Names(_)) => {
                let mut cells = Mask::new(w, h);
                let runs: Vec<_> = placed.runs.iter().filter(|r| r.layer == i).collect();
                let marks: Vec<_> = placed.marks.iter().filter(|m| m.layer == i).collect();
                for run in &runs {
                    for k in 0..run.text.len() {
                        let (x, y) = letter_at(run, k, (1, 1), true);
                        cells.set(x, y);
                    }
                }
                for m in &marks {
                    cells.set(m.x, m.y);
                }
                clear_halo(&mut text, &cells, layer);
                for run in &runs {
                    for (k, &c) in run.text.iter().enumerate() {
                        let (x, y) = letter_at(run, k, (1, 1), true);
                        if let Some(cell) = text.at(x, y) {
                            cell.kind = Kind::Char(c);
                            cell.fg = t.color;
                            if t.background.is_some() {
                                cell.bg = t.background;
                            }
                        }
                    }
                }
                for m in &marks {
                    let c = match m.marker {
                        Marker::Char(c) => c,
                        Marker::Dot(_) => '•',
                    };
                    if let Some(cell) = text.at(m.x, m.y) {
                        cell.kind = Kind::Char(c);
                        cell.fg = t.color;
                    }
                }
            }
            _ => {}
        }
    }
    text
}

/// The cells with at least `least` of their dots set.
fn cells_of(dots: &Mask, w: usize, h: usize, least: usize) -> Mask {
    let mut count = vec![0u8; w * h];
    for (x, y) in dots.dots() {
        let (cx, cy) = (x / CELL_DOTS.0 as usize, y / CELL_DOTS.1 as usize);
        if cx < w && cy < h {
            count[cy * w + cx] += 1;
        }
    }
    let mut out = Mask::new(w, h);
    for (i, n) in count.iter().enumerate() {
        if *n as usize >= least {
            out.set((i % w) as i64, (i / w) as i64);
        }
    }
    out
}

fn clear_halo(text: &mut Text, cells: &Mask, layer: &Painted) {
    if layer.halo.is_none() {
        return;
    }
    for (x, y) in cells.grown(layer.halo.0, layer.halo.1, false).dots() {
        if let Some(cell) = text.at(x as i64, y as i64) {
            cell.kind = Kind::Blank;
            cell.fg = None;
        }
    }
}

fn fill(text: &mut Text, mask: &Mask, f: &FillPaint, origin: (i64, i64)) {
    let (w, h) = (text.width, text.height);
    let (dx, dy) = (CELL_DOTS.0 as usize, CELL_DOTS.1 as usize);
    let inside = cells_of(mask, w, h, (dx * dy) / 2);
    if let Some(bg) = f.background {
        for (x, y) in inside.dots() {
            if let Some(cell) = text.at(x as i64, y as i64) {
                cell.bg = Some(bg);
            }
        }
    }
    let pattern = f.pattern.as_deref().and_then(Pattern::named);
    if pattern.is_some() || f.tone.is_some() {
        // Braille: each dot of the area inked as the pattern says.
        let touched = cells_of(mask, w, h, 1);
        for (cx, cy) in touched.dots() {
            let (mut inked, mut covered) = (0u8, 0u8);
            for y in 0..dy {
                for x in 0..dx {
                    let (px, py) = ((cx * dx + x) as i64, (cy * dy + y) as i64);
                    if !mask.get(px, py) {
                        continue;
                    }
                    let bit = braille_bit(x, y);
                    covered |= bit;
                    let (wx, wy) = (origin.0 + px, origin.1 + py);
                    let on = match pattern {
                        Some(p) => p.at(wx, wy),
                        None => tone_at(f.tone.unwrap_or(0.0), wx, wy),
                    };
                    if on {
                        inked |= bit;
                    }
                }
            }
            let cell = text.at(cx as i64, cy as i64).expect("a touched cell is on the grid");
            let kept = match cell.kind {
                Kind::Braille(b) if cell.fg == f.color => b & !covered,
                _ => 0,
            };
            if f.opaque {
                if inked | kept != 0 {
                    cell.kind = Kind::Braille(inked | kept);
                    cell.fg = f.color;
                } else if covered.count_ones() as usize * 2 >= dx * dy {
                    cell.kind = Kind::Blank;
                }
            } else if inked != 0 {
                cell.kind = match cell.kind {
                    Kind::Braille(b) if cell.fg == f.color => Kind::Braille(b | inked),
                    _ => Kind::Braille(inked),
                };
                cell.fg = f.color;
            }
        }
        return;
    }
    if f.blocks {
        let touched = cells_of(mask, w, h, 1);
        for (cx, cy) in touched.dots() {
            let mut quarters = 0u8;
            for (q, (qx, qy)) in [(0, 0), (1, 0), (0, 2), (1, 2)].into_iter().enumerate() {
                let (px, py) = ((cx * dx + qx) as i64, (cy * dy + qy) as i64);
                if mask.get(px, py) || mask.get(px, py + 1) {
                    quarters |= 1 << q;
                }
            }
            let cell = text.at(cx as i64, cy as i64).expect("on the grid");
            cell.kind = Kind::Char(QUARTERS[quarters as usize]);
            cell.fg = f.color;
        }
        return;
    }
    if let Some(tile) = &f.chars {
        let (ox, oy) = (origin.0.div_euclid(dx as i64), origin.1.div_euclid(dy as i64));
        for (cx, cy) in inside.dots() {
            let row = &tile.0[(oy + cy as i64).rem_euclid(tile.0.len() as i64) as usize];
            let c = row[(ox + cx as i64).rem_euclid(row.len() as i64) as usize];
            let cell = text.at(cx as i64, cy as i64).expect("on the grid");
            if c == ' ' && !f.opaque {
                continue;
            }
            cell.kind = if c == ' ' { Kind::Blank } else { Kind::Char(c) };
            cell.fg = f.color;
        }
    }
}

/// Degrees off the level or the upright past which a box line is slanted.
const SLANT: f64 = 20.0;

/// Lines in cell units, cut to the grid with a cell to spare.
fn cell_segments(lines: &[geo::LineString<f64>], w: usize, h: usize) -> Vec<Vec<((f64, f64), (f64, f64))>> {
    let (ux, uy) = CELL;
    let (lo, hi) = ((-1.0, -1.0), (w as f64 + 1.0, h as f64 + 1.0));
    lines
        .iter()
        .map(|l| {
            l.0.windows(2)
                .filter_map(|p| clip((p[0].x / ux, p[0].y / uy), (p[1].x / ux, p[1].y / uy), lo, hi))
                .collect()
        })
        .collect()
}

/// The cells a line set crosses.
fn line_cells(lines: &[geo::LineString<f64>], l: &LinePaint, w: usize, h: usize) -> Mask {
    let mut cells = Mask::new(w, h);
    if l.style == LineStyle::Braille && l.char.is_none() {
        let dots = ToDots { per_dot: DOT };
        for line in lines {
            for run in line_dots(line, dots, w * CELL_DOTS.0 as usize, h * CELL_DOTS.1 as usize) {
                for (x, y) in run {
                    cells.set(x.div_euclid(CELL_DOTS.0 as i64), y.div_euclid(CELL_DOTS.1 as i64));
                }
            }
        }
        return cells;
    }
    for segments in cell_segments(lines, w, h) {
        for (a, b) in segments {
            cell_walk(a, b, |x, y| cells.set(x, y));
        }
    }
    cells
}

fn line(text: &mut Text, lines: &[geo::LineString<f64>], l: &LinePaint, w: usize, h: usize) {
    let paint_bg = |cell: &mut Cell| {
        if l.background.is_some() {
            cell.bg = l.background;
        }
    };
    if let Some(c) = l.char {
        for (x, y) in line_cells(lines, l, w, h).dots() {
            let cell = text.at(x as i64, y as i64).expect("on the grid");
            cell.kind = Kind::Char(c);
            cell.fg = l.color;
            paint_bg(cell);
        }
        return;
    }
    match l.style {
        LineStyle::Braille => {
            let dots = ToDots { per_dot: DOT };
            for line in lines {
                for run in line_dots(line, dots, w * CELL_DOTS.0 as usize, h * CELL_DOTS.1 as usize) {
                    for (x, y) in run {
                        text.dot(x, y, l.color);
                        if let Some(cell) = text.at(x.div_euclid(CELL_DOTS.0 as i64), y.div_euclid(CELL_DOTS.1 as i64)) {
                            paint_bg(cell);
                        }
                    }
                }
            }
        }
        LineStyle::Ascii => {
            let (ux, uy) = CELL;
            let mut strokes: std::collections::HashMap<(i64, i64), u8> = Default::default();
            for segments in cell_segments(lines, w, h) {
                for (a, b) in segments {
                    // The stroke by the segment's slope on the screen.
                    let angle = ((b.1 - a.1) * uy).atan2((b.0 - a.0) * ux).to_degrees().rem_euclid(180.0);
                    let s = if !(22.5..157.5).contains(&angle) {
                        1
                    } else if (67.5..112.5).contains(&angle) {
                        2
                    } else if angle < 90.0 {
                        8
                    } else {
                        4
                    };
                    cell_walk(a, b, |x, y| *strokes.entry((x, y)).or_default() |= s);
                }
            }
            for ((x, y), s) in strokes {
                if let Some(cell) = text.at(x, y) {
                    cell.kind = match cell.kind {
                        Kind::Ascii(old) if cell.fg == l.color => Kind::Ascii(old | s),
                        _ => Kind::Ascii(s),
                    };
                    cell.fg = l.color;
                    paint_bg(cell);
                }
            }
        }
        style => {
            // Box drawing: each cell keeps the sides its line leaves it by.
            // A stretch running neither across nor down would be a
            // staircase of corners; it is drawn in Braille dots instead.
            let (ux, uy) = CELL;
            let mut sides: std::collections::HashMap<(i64, i64), u8> = Default::default();
            let mut slanted: Vec<geo::LineString<f64>> = Vec::new();
            for segments in cell_segments(lines, w, h) {
                let mut prev: Option<(i64, i64)> = None;
                let mut last_end: Option<(f64, f64)> = None;
                for (a, b) in segments {
                    if last_end.is_some_and(|e| (e.0 - a.0).abs() > 1e-9 || (e.1 - a.1).abs() > 1e-9) {
                        prev = None;
                    }
                    let angle = ((b.1 - a.1) * uy).abs().atan2(((b.0 - a.0) * ux).abs()).to_degrees();
                    if (SLANT..90.0 - SLANT).contains(&angle) {
                        slanted.push(geo::LineString::from(vec![(a.0 * ux, a.1 * uy), (b.0 * ux, b.1 * uy)]));
                        prev = None;
                        last_end = Some(b);
                        continue;
                    }
                    let horizontal = (b.0 - a.0).abs() >= (b.1 - a.1).abs();
                    cell_walk(a, b, |x, y| {
                        let here = sides.entry((x, y)).or_default();
                        if *here == 0 {
                            // A lone cell still shows which way it runs.
                            *here |= 0x10 | if horizontal { 0x20 } else { 0 };
                        }
                        if let Some((px, py)) = prev {
                            let side = match (x - px, y - py) {
                                (1, 0) => Some((2u8, 8u8)),
                                (-1, 0) => Some((8, 2)),
                                (0, 1) => Some((4, 1)),
                                (0, -1) => Some((1, 4)),
                                _ => None,
                            };
                            if let Some((from, to)) = side {
                                *sides.entry((px, py)).or_default() |= from;
                                *sides.entry((x, y)).or_default() |= to;
                            }
                        }
                        prev = Some((x, y));
                    });
                    last_end = Some(b);
                }
            }
            for ((x, y), s) in sides {
                let mut joins = s & 15;
                if joins == 0 {
                    joins = if s & 0x20 != 0 { 2 | 8 } else { 1 | 4 };
                }
                if let Some(cell) = text.at(x, y) {
                    cell.kind = match cell.kind {
                        Kind::Box(old, j) if old == style && cell.fg == l.color => Kind::Box(style, j | joins),
                        _ => Kind::Box(style, joins),
                    };
                    cell.fg = l.color;
                    paint_bg(cell);
                }
            }
            let dots = ToDots { per_dot: DOT };
            for line in &slanted {
                for run in line_dots(line, dots, w * CELL_DOTS.0 as usize, h * CELL_DOTS.1 as usize) {
                    for (x, y) in run {
                        let cell = (x.div_euclid(CELL_DOTS.0 as i64), y.div_euclid(CELL_DOTS.1 as i64));
                        if text.at(cell.0, cell.1).is_some_and(|c| matches!(c.kind, Kind::Box(..))) {
                            continue;
                        }
                        text.dot(x, y, l.color);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_lines_join_where_they_cross() {
        assert_eq!(box_char(LineStyle::Box, 1 | 2 | 4 | 8), '┼');
        assert_eq!(box_char(LineStyle::Double, 2 | 4), '╔');
        assert_eq!(box_char(LineStyle::Heavy, 2 | 8), '━');
    }

    #[test]
    fn braille_dots_add_up_in_a_cell() {
        let mut t = Text::new(1, 1);
        t.dot(0, 0, None);
        t.dot(1, 3, None);
        assert_eq!(t.cell(0, 0).char(), '⢁');
    }
}
