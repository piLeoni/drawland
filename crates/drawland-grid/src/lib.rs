//! draw.land on a screen. The graph's `paint` entries, evaluated by the
//! engine ([`drawland_core::grammar_run::paint`]), are painted into a grid:
//! a 1-bit bitmap (black and white pixels), or
//! character cells for a terminal. Each layer is painted over the ones
//! before, its `halo` cleared out of them first; names are placed in whole
//! pixels or cells, none over another.
//!
//! ```no_run
//! # fn layers() -> Vec<drawland_core::grammar_run::Painted> { vec![] }
//! # let output: drawland_core::output::Output = unimplemented!();
//! # let screen: drawland_core::paint::Screen = unimplemented!();
//! let picture = drawland_grid::render(&screen, drawland_grid::origin(&output, &screen), &layers());
//! std::fs::write("map.ans", picture.encode(drawland_grid::Format::Ansi, "map")?)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod bitmap;
pub mod font;
pub mod incognita;
pub mod pattern;
pub mod place;
pub mod raster;
pub mod scene;
pub mod text;
pub mod ui;

pub use bitmap::Bitmap;
pub use scene::{Recall, Scene, View};
pub use text::Text;

use drawland_core::grammar_run::{PaintShape, Painted};
use drawland_core::output::Output;
use drawland_core::paint::{Look, Screen, ScreenKind};

/// A painted screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Picture {
    Bitmap(Bitmap),
    Text(Text),
}

/// How a picture is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A 1-bit PNG (a bitmap), or a picture of the cells (text).
    Png,
    /// A binary PBM (a bitmap).
    Pbm,
    /// The characters alone.
    Txt,
    /// The characters in colour, for a terminal.
    Ansi,
    /// A web page of the characters in colour.
    Html,
}

impl Format {
    /// The format a file's extension asks: `png`, `pbm`, `txt`, `ans`, `html`.
    pub fn from_extension(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "png" => Format::Png,
            "pbm" => Format::Pbm,
            "txt" => Format::Txt,
            "ans" | "ansi" => Format::Ansi,
            "html" | "htm" => Format::Html,
            _ => return None,
        })
    }

    /// The extension a screen is written with when nothing says.
    pub fn default_for(kind: ScreenKind) -> Format {
        match kind {
            ScreenKind::Bitmap => Format::Png,
            ScreenKind::Text => Format::Ansi,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Pbm => "pbm",
            Format::Txt => "txt",
            Format::Ansi => "ans",
            Format::Html => "html",
        }
    }
}

impl Picture {
    /// The picture as `format` writes it. A bitmap written as text is drawn
    /// in Braille; text written as PNG is a picture of the terminal.
    pub fn encode(&self, format: Format, title: &str) -> Result<Vec<u8>, String> {
        match (self, format) {
            (Picture::Bitmap(b), Format::Png) => b.png(),
            (Picture::Bitmap(b), Format::Pbm) => Ok(b.pbm()),
            (Picture::Bitmap(b), f) => Picture::Text(b.braille()).encode(f, title),
            (Picture::Text(t), Format::Txt) => Ok(t.txt().into_bytes()),
            (Picture::Text(t), Format::Ansi) => Ok(t.ansi().into_bytes()),
            (Picture::Text(t), Format::Html) => Ok(t.html(title).into_bytes()),
            (Picture::Text(t), Format::Png) => t.png(),
            (Picture::Text(_), f) => Err(format!("a text screen is written as txt, ans, html or png, not {}", f.extension())),
        }
    }

    /// A globe's picture (an `ortho` output) with everything past its rim
    /// cleared, the rim drawn: the globe is `radius` units round the
    /// screen's centre. Past the rim the profile's background (a page
    /// filled as land) would stand for ground there is none of. With
    /// `names` (the same screen's names painted alone), the names run on
    /// past the rim instead of being cut by it, and the rim gives way to
    /// them.
    pub fn globe(&mut self, radius: f64, names: Option<&Picture>) {
        match (self, names) {
            (Picture::Bitmap(b), names) => {
                let names = match names {
                    Some(Picture::Bitmap(n)) if n.width == b.width && n.height == b.height => Some(n),
                    _ => None,
                };
                // A name's ink, and a pixel round it (its halo): the rim
                // stops short of it.
                let ink = |x: usize, y: usize| names.is_some_and(|n| n.black(x, y) != n.invert);
                let (w, h) = (b.width, b.height);
                let near = |x: usize, y: usize| {
                    (x.saturating_sub(1)..=(x + 1).min(w - 1)).any(|i| (y.saturating_sub(1)..=(y + 1).min(h - 1)).any(|j| ink(i, j)))
                };
                let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
                for y in 0..h {
                    for x in 0..w {
                        let d = (x as f64 + 0.5 - cx).hypot(y as f64 + 0.5 - cy);
                        if d > radius + 0.5 {
                            b.put(x, y, ink(x, y));
                        } else if d > radius - 0.5 && !near(x, y) {
                            b.put(x, y, true);
                        }
                    }
                }
            }
            (Picture::Text(t), names) => {
                let names = match names {
                    Some(Picture::Text(n)) if n.width == t.width && n.height == t.height => Some(n),
                    _ => None,
                };
                // A cell is one unit across and two down.
                let (cx, cy) = (t.width as f64 / 2.0, t.height as f64);
                for y in 0..t.height {
                    for x in 0..t.width {
                        let d = (x as f64 + 0.5 - cx).hypot((y as f64 + 0.5) * 2.0 - cy);
                        if d > radius + 0.5 {
                            match names {
                                Some(n) => t.put(x, y, n.cell(x, y).clone()),
                                None => t.clear(x, y),
                            }
                        }
                    }
                }
                // The rim in Braille dots, two to a unit, on empty cells.
                let steps = (radius * 4.0 * std::f64::consts::PI).ceil().max(16.0) as usize;
                let rim = drawland_core::paint::Rgb(0x5a, 0x64, 0x78);
                for i in 0..steps {
                    let a = i as f64 / steps as f64 * std::f64::consts::TAU;
                    let (x, y) = ((cx + radius * a.cos()) * 2.0, (cy + radius * a.sin()) * 2.0);
                    let cell = ((x / 2.0) as usize, (y / 4.0) as usize);
                    let named = names.is_some_and(|n| n.cell(cell.0.min(n.width - 1), cell.1.min(n.height - 1)).char() != ' ');
                    if cell.0 < t.width && cell.1 < t.height && t.cell(cell.0, cell.1).char() == ' ' && !named {
                        t.dot(x as i64, y as i64, Some(rim));
                    }
                }
            }
        }
    }

    /// The picture as a terminal shows it: text in colour, a bitmap in
    /// Braille.
    pub fn ansi(&self) -> String {
        match self {
            Picture::Bitmap(b) => b.braille().ansi(),
            Picture::Text(t) => t.ansi(),
        }
    }
}

/// Where the screen's top-left dot is in the world's dots: what patterns
/// and tiled characters are read against, so they stay on the ground as
/// the map moves ([`View::origin`]). Off web Mercator (or turned), they are
/// read from the screen's corner.
pub fn origin(output: &Output, screen: &Screen) -> (i64, i64) {
    View::of(output, *screen).map(|v| v.origin()).unwrap_or((0, 0))
}

/// The layers, bottom first, painted on `screen`; `origin` from [`origin`].
pub fn render(screen: &Screen, origin: (i64, i64), layers: &[Painted]) -> Picture {
    render_hinted(screen, origin, layers, &place::Hints::new()).0
}

/// [`render`], each name first tried where `hints` say it was; with the
/// names as placed.
pub fn render_hinted(screen: &Screen, origin: (i64, i64), layers: &[Painted], hints: &place::Hints) -> (Picture, place::Placed) {
    let cells = screen.kind == ScreenKind::Text;
    let unit = screen.cell();
    let size = (screen.width as i64, screen.height as i64);
    let texts: Vec<place::Layer> = layers
        .iter()
        .enumerate()
        .filter_map(|(index, l)| match (&l.look, &l.shape) {
            (Look::Text(t), PaintShape::Names(names)) => {
                let letter = if cells {
                    (1, 1)
                } else {
                    let f = font::font(&t.font)?;
                    (f.width as i64, f.height as i64)
                };
                Some(place::Layer { index, paint: t, names, metrics: place::Metrics { letter, unit, size } })
            }
            _ => None,
        })
        .collect();
    let placed = place::place(&texts, size, hints);
    let picture = match screen.kind {
        ScreenKind::Bitmap => {
            Picture::Bitmap(bitmap::paint(layers, &placed, screen.width as usize, screen.height as usize, screen.invert, origin))
        }
        ScreenKind::Text => Picture::Text(text::paint(layers, &placed, screen.width as usize, screen.height as usize, origin)),
    };
    (picture, placed)
}
