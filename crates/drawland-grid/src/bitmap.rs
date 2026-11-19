//! The 1-bit screen: every pixel black or white, painted a layer at a time,
//! each layer's halo cleared out of the ones under it first. Written as PNG
//! (1 bit a pixel) or PBM, or shown in a terminal in Braille.

use crate::font::font;
use crate::pattern::{tone_at, Pattern};
use crate::place::{letter_at, Placed};
use crate::raster::{area_mask, line_dots, Mask, ToDots};
use crate::text::Text;
use drawland_core::grammar_run::{PaintShape, Painted};
use drawland_core::paint::{FillPaint, Ink, LinePaint, Look, Marker, PIXEL_MM};

#[derive(Clone, Debug, PartialEq)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    /// True where the pixel is black.
    black: Vec<bool>,
    /// Written white on black.
    pub invert: bool,
}

impl Bitmap {
    pub fn new(width: usize, height: usize, invert: bool) -> Bitmap {
        Bitmap { width, height, black: vec![false; width * height], invert }
    }

    /// Whether the pixel shows black (after `invert`).
    pub fn black(&self, x: usize, y: usize) -> bool {
        self.black[y * self.width + x] != self.invert
    }

    fn set(&mut self, x: usize, y: usize, ink: Ink) {
        self.black[y * self.width + x] = ink == Ink::Black;
    }

    /// A 1-bit greyscale PNG.
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.width as u32, self.height as u32);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::One);
            let mut writer = enc.write_header().map_err(|e| format!("png: {e}"))?;
            let stride = self.width.div_ceil(8);
            let mut data = vec![0u8; stride * self.height];
            for y in 0..self.height {
                for x in 0..self.width {
                    // Greyscale: a set bit is white.
                    if !self.black(x, y) {
                        data[y * stride + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            writer.write_image_data(&data).map_err(|e| format!("png: {e}"))?;
        }
        Ok(out)
    }

    /// A binary PBM (P4): a set bit is black.
    pub fn pbm(&self) -> Vec<u8> {
        let mut out = format!("P4\n{} {}\n", self.width, self.height).into_bytes();
        let stride = self.width.div_ceil(8);
        let mut row = vec![0u8; stride];
        for y in 0..self.height {
            row.iter_mut().for_each(|b| *b = 0);
            for x in 0..self.width {
                if self.black(x, y) {
                    row[x / 8] |= 0x80 >> (x % 8);
                }
            }
            out.extend_from_slice(&row);
        }
        out
    }

    /// The bitmap in Braille, a cell for every 2 × 4 pixels: how a terminal
    /// shows it.
    pub fn braille(&self) -> Text {
        let (cw, ch) = (self.width.div_ceil(2), self.height.div_ceil(4));
        let mut text = Text::new(cw, ch);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.black(x, y) {
                    text.dot(x as i64, y as i64, None);
                }
            }
        }
        text
    }
}

/// The ink a layer lays and the ink its halo clears to.
fn other(ink: Ink) -> Ink {
    match ink {
        Ink::Black => Ink::White,
        Ink::White => Ink::Black,
    }
}

/// The layers painted on a `w` × `h` bitmap whose top-left pixel is
/// `origin` in the world's pixels (where the patterns are read).
pub fn paint(layers: &[Painted], placed: &Placed, w: usize, h: usize, invert: bool, origin: (i64, i64)) -> Bitmap {
    let to = ToDots { mm_per_dot: PIXEL_MM };
    let mut bmp = Bitmap::new(w, h, invert);
    for (i, layer) in layers.iter().enumerate() {
        match (&layer.look, &layer.shape) {
            (Look::Fill(f), PaintShape::Area(a)) => {
                let mask = area_mask(a, to, w, h);
                clear_halo(&mut bmp, &mask, layer, f.ink);
                fill(&mut bmp, &mask, f, origin);
            }
            (Look::Line(l), PaintShape::Lines(lines)) => {
                let mask = line_mask(lines, l, to, w, h);
                clear_halo(&mut bmp, &mask, layer, l.ink);
                lay(&mut bmp, &mask, l.ink);
            }
            (Look::Text(t), PaintShape::Names(_)) => {
                let Some(f) = font(&t.font) else { continue };
                let mut mask = Mask::new(w, h);
                for run in placed.runs.iter().filter(|r| r.layer == i) {
                    for (k, &c) in run.text.iter().enumerate() {
                        let (bx, by) = letter_at(run, k, (f.width as i64, f.height as i64), false);
                        for gy in 0..f.height {
                            for gx in 0..f.width {
                                if f.ink(c, gx, gy) {
                                    let (x, y) = if run.down {
                                        (bx + (f.height - 1 - gy) as i64, by + gx as i64)
                                    } else {
                                        (bx + gx as i64, by + gy as i64)
                                    };
                                    mask.set(x, y);
                                }
                            }
                        }
                    }
                }
                for mark in placed.marks.iter().filter(|m| m.layer == i) {
                    let d = match mark.marker {
                        Marker::Dot(d) => d as i64,
                        Marker::Char(_) => 1,
                    };
                    let r = d as f64 / 2.0;
                    for y in 0..d {
                        for x in 0..d {
                            let (dx, dy) = (x as f64 + 0.5 - r, y as f64 + 0.5 - r);
                            if dx * dx + dy * dy <= r * r + 0.25 {
                                mask.set(mark.x + x, mark.y + y);
                            }
                        }
                    }
                }
                clear_halo(&mut bmp, &mask, layer, t.ink);
                lay(&mut bmp, &mask, t.ink);
            }
            _ => {}
        }
    }
    bmp
}

fn clear_halo(bmp: &mut Bitmap, ink: &Mask, layer: &Painted, laid: Ink) {
    if layer.halo.is_none() {
        return;
    }
    let halo = ink.grown(layer.halo.0, layer.halo.1, true);
    lay(bmp, &halo, other(laid));
}

fn lay(bmp: &mut Bitmap, mask: &Mask, ink: Ink) {
    for (x, y) in mask.dots() {
        bmp.set(x, y, ink);
    }
}

fn fill(bmp: &mut Bitmap, area: &Mask, f: &FillPaint, origin: (i64, i64)) {
    let pattern = f.pattern.as_deref().and_then(Pattern::named);
    for (x, y) in area.dots() {
        let (wx, wy) = (origin.0 + x as i64, origin.1 + y as i64);
        let on = match (pattern, f.tone) {
            (Some(p), _) => p.at(wx, wy),
            (None, Some(t)) => tone_at(t, wx, wy),
            // Characters, blocks or a background alone: on pixels, solid.
            (None, None) => true,
        };
        if on {
            bmp.set(x, y, f.ink);
        } else if f.opaque {
            bmp.set(x, y, other(f.ink));
        }
    }
}

/// The pixels lines cover: a line a pixel wide stepped dot to dot, widened
/// by a round brush, cut into dashes along each run.
pub fn line_mask(lines: &[geo::LineString<f64>], l: &LinePaint, to: ToDots, w: usize, h: usize) -> Mask {
    let mut mask = Mask::new(w, h);
    let width = l.width as i64;
    let lo = -(width - 1) / 2;
    let hi = width / 2;
    let r = width as f64 / 2.0;
    let brush: Vec<(i64, i64)> = (lo..=hi)
        .flat_map(|dy| (lo..=hi).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy)| {
            if width <= 2 {
                return true;
            }
            let (fx, fy) = (dx as f64 + 0.5 - (width % 2) as f64 * 0.5, dy as f64 + 0.5 - (width % 2) as f64 * 0.5);
            fx * fx + fy * fy <= r * r + 0.5
        })
        .collect();
    for line in lines {
        for run in line_dots(line, to, w, h) {
            for (k, &(x, y)) in run.iter().enumerate() {
                if let Some([on, off]) = l.dash {
                    if (k as u32) % (on + off) >= on {
                        continue;
                    }
                }
                for &(dx, dy) in &brush {
                    mask.set(x + dx, y + dy);
                }
            }
        }
    }
    mask
}
