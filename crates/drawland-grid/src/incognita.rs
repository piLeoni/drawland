//! Terra incognita: where a moving screen has no map yet (zoomed out faster
//! than a wider scene is built), the sea, since where there is no land
//! there is sea, with rings running out from the screen's middle as a
//! sounding would, while the land is found: on text thin bright rings, on
//! a bitmap bands as wide as the gaps between them, the sea inverted in
//! them.
//!
//! The sea is the style's own (its `water` layer's look), so the unknown
//! reads as more of the sea there is; the rings move with the time a frame
//! is painted, so a page that asks for frames while a scene is built (it
//! does) sees them run.

use crate::{Bitmap, Picture, Text};
use drawland_core::grammar_run::{PaintShape, Painted};
use drawland_core::paint::{FillPaint, Look, Screen};
use geo::{LineString, MultiPolygon, Polygon};

/// Rings on text: a ring this many units apart, so wide.
const PERIOD: f64 = 14.0;
const WIDTH: f64 = 1.6;
/// Bands on a bitmap: one this many pixels wide, a gap as wide.
const BAND: f64 = 12.0;
/// Either running out this fast, units a second.
const SPEED: f64 = 22.0;

/// The look the unknown is painted in: the `water` layer's fill, else a
/// sea of waves.
pub fn sea(layers: &[Painted]) -> Look {
    layers
        .iter()
        .find(|l| l.id == "water" && matches!(l.look, Look::Fill(_)))
        .map(|l| l.look.clone())
        .unwrap_or_else(|| Look::Fill(FillPaint { pattern: Some("waves".into()), ..FillPaint::default() }))
}

/// `picture` with the sea, and its rings at `t` seconds, wherever
/// `unknown(x, y)` (a cell's or pixel's middle, in units) holds.
pub fn fill(picture: &mut Picture, screen: &Screen, origin: (i64, i64), look: &Look, unknown: impl Fn(f64, f64) -> bool, t: f64) {
    let (vw, vh) = screen.size();
    let (cw, ch) = screen.cell();
    let (cols, rows) = (screen.width as usize, screen.height as usize);
    let middle = |x: usize, y: usize| ((x as f64 + 0.5) * cw, (y as f64 + 0.5) * ch);
    if !(0..rows).any(|y| (0..cols).any(|x| unknown(middle(x, y).0, middle(x, y).1))) {
        return;
    }
    let all = Polygon::new(LineString::from(vec![(0.0, 0.0), (vw, 0.0), (vw, vh), (0.0, vh), (0.0, 0.0)]), vec![]);
    let sea = Painted { id: "incognita".into(), look: look.clone(), halo: drawland_core::paint::Halo(0, 0), shape: PaintShape::Area(MultiPolygon::new(vec![all])) };
    let painted = crate::render(screen, origin, std::slice::from_ref(&sea));
    let (period, width) = match screen.kind {
        drawland_core::paint::ScreenKind::Text => (PERIOD, WIDTH),
        drawland_core::paint::ScreenKind::Bitmap => (2.0 * BAND, BAND),
    };
    let ring = |x: f64, y: f64| {
        let d = (x - vw / 2.0).hypot(y - vh / 2.0);
        (d - t * SPEED).rem_euclid(period) < width
    };
    match (picture, &painted) {
        (Picture::Text(text), Picture::Text(sea)) => sound_text(text, sea, |x, y| unknown(middle(x, y).0, middle(x, y).1), |x, y| ring(middle(x, y).0, middle(x, y).1)),
        (Picture::Bitmap(b), Picture::Bitmap(sea)) => sound_bitmap(b, sea, |x, y| unknown(middle(x, y).0, middle(x, y).1), |x, y| ring(middle(x, y).0, middle(x, y).1)),
        _ => {}
    }
}

fn sound_text(text: &mut Text, sea: &Text, unknown: impl Fn(usize, usize) -> bool, ring: impl Fn(usize, usize) -> bool) {
    for y in 0..text.height {
        for x in 0..text.width {
            if unknown(x, y) {
                text.put(x, y, *sea.cell(x, y));
                if ring(x, y) {
                    text.glint(x, y);
                }
            }
        }
    }
}

fn sound_bitmap(b: &mut Bitmap, sea: &Bitmap, unknown: impl Fn(usize, usize) -> bool, ring: impl Fn(usize, usize) -> bool) {
    for y in 0..b.height {
        for x in 0..b.width {
            if unknown(x, y) {
                // Raw, before the screen's inversion, as `put` takes it.
                let ink = sea.black(x, y) != sea.invert;
                b.put(x, y, ink != ring(x, y));
            }
        }
    }
}

