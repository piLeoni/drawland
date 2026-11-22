//! Names placed on the grid: in whole pixels or cells, level (or, along a
//! road that runs down the screen, one letter under the other), none over
//! another. The layer of the largest letters is placed first (the top one
//! of those alike), then each layer's names in the order of importance
//! `labels::named` gives: a city's name before a quarter's.
//!
//! Where a name was in the last picture ([`Hints`]) is tried first, so a
//! name on a road stays where it was on the ground as the map zooms, rather
//! than hopping between the places along the road it could go.

use crate::raster::Mask;
use drawland_core::labels::{Named, Shape};
use drawland_core::paint::{Marker, TextPaint};
use geo::{Contains, Coord, InteriorPoint, Point};
use std::collections::HashMap;

/// One name written: its letters from `(x, y)`, the top-left of the first,
/// in grid units (pixels or cells), across, or down when `down`.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub layer: usize,
    pub text: Vec<char>,
    pub x: i64,
    pub y: i64,
    pub down: bool,
    /// The middle of its box, in grid units.
    pub at: (f64, f64),
}

/// Where names were: by layer (its id) and text, the middles of their
/// boxes, in grid units of the picture being placed.
pub type Hints = HashMap<(String, String), Vec<(f64, f64)>>;

/// A place's mark: its box's top-left, in grid units.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub layer: usize,
    pub x: i64,
    pub y: i64,
    pub marker: Marker,
}

#[derive(Clone, Debug, Default)]
pub struct Placed {
    pub runs: Vec<Run>,
    pub marks: Vec<Mark>,
}

/// How a letter sits on the grid.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// A letter's size in grid units (a font's pixels, or one cell).
    pub letter: (i64, i64),
    /// One grid unit in the output's units, across and down (a pixel is
    /// one by one, a character one by two).
    pub unit: (f64, f64),
    /// The grid's size in units.
    pub size: (i64, i64),
}

/// One text layer to place.
pub struct Layer<'a> {
    pub index: usize,
    pub id: &'a str,
    pub paint: &'a TextPaint,
    pub names: &'a [Named],
    pub metrics: Metrics,
}

#[derive(Clone, Copy, Debug)]
struct Box {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

impl Box {
    fn grown(&self, p: i64) -> Box {
        Box { x: self.x - p, y: self.y - p, w: self.w + 2 * p, h: self.h + 2 * p }
    }

    fn inside(&self, size: (i64, i64)) -> bool {
        self.x >= 0 && self.y >= 0 && self.x + self.w <= size.0 && self.y + self.h <= size.1
    }

    fn hits(&self, taken: &Mask) -> bool {
        (self.y..self.y + self.h).any(|y| (self.x..self.x + self.w).any(|x| taken.get(x, y)))
    }

    fn take(&self, taken: &mut Mask) {
        for y in self.y..self.y + self.h {
            for x in self.x..self.x + self.w {
                taken.set(x, y);
            }
        }
    }

    fn center(&self) -> (f64, f64) {
        (self.x as f64 + self.w as f64 / 2.0, self.y as f64 + self.h as f64 / 2.0)
    }
}

/// Every layer's names: the largest letters first, then the last layer
/// first. `size` is the grid's in units.
pub fn place(layers: &[Layer], size: (i64, i64), hints: &Hints) -> Placed {
    let mut taken = Mask::new(size.0.max(0) as usize, size.1.max(0) as usize);
    let mut out = Placed::default();
    let mut order: Vec<&Layer> = layers.iter().collect();
    order.sort_by_key(|l| (std::cmp::Reverse(l.metrics.letter.0 * l.metrics.letter.1), std::cmp::Reverse(l.index)));
    for layer in order {
        let mut seen: HashMap<&str, Vec<(f64, f64)>> = HashMap::new();
        let was = |named: &Named| hints.get(&(layer.id.to_string(), named.text.clone())).map(Vec::as_slice).unwrap_or(&[]);
        // First every name where it was (a road's name is on every piece of
        // the road: the piece it was on must not lose it to one before it),
        // then the rest, as they come.
        let mut done = vec![false; layer.names.len()];
        for (i, named) in layer.names.iter().enumerate() {
            if !was(named).is_empty() {
                done[i] = place_one(layer, named, was(named), true, &mut taken, &mut seen, &mut out);
            }
        }
        for (i, named) in layer.names.iter().enumerate() {
            if !done[i] || matches!(named.shape, Shape::Road(_)) {
                place_one(layer, named, &[], false, &mut taken, &mut seen, &mut out);
            }
        }
    }
    out
}

/// One name placed, if it goes; whether it went. `only_was`: a road's name
/// only where it was (`was`), nowhere else.
fn place_one<'a>(layer: &Layer, named: &'a Named, was: &[(f64, f64)], only_was: bool, taken: &mut Mask, seen: &mut HashMap<&'a str, Vec<(f64, f64)>>, out: &mut Placed) -> bool {
    let m = layer.metrics;
    let p = layer.paint;
    let text: Vec<char> = named.text.chars().filter(|c| !c.is_control()).collect();
    if text.is_empty() {
        return false;
    }
    let n = text.len() as i64;
    let (lw, lh) = m.letter;
    let across = (n * lw, lh);
    let pad = p.padding as i64;
    let spacing = p.spacing as f64;
    let units = |c: Coord<f64>| (c.x / m.unit.0, c.y / m.unit.1);
    let far = |b: &Box, seen: &HashMap<&str, Vec<(f64, f64)>>| {
        let (cx, cy) = b.center();
        seen.get(named.text.as_str()).is_none_or(|v| v.iter().all(|&(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() >= spacing))
    };
    let fits = |b: &Box, taken: &Mask| b.inside(m.size) && !b.grown(pad).hits(taken);
    let level = |cx: f64, cy: f64| Box { x: (cx - across.0 as f64 / 2.0).round() as i64, y: (cy - across.1 as f64 / 2.0).round() as i64, w: across.0, h: across.1 };
    let mut keep = |b: Box, down: bool, taken: &mut Mask, seen: &mut HashMap<&'a str, Vec<(f64, f64)>>, out: &mut Placed| {
        b.grown(pad).take(taken);
        seen.entry(named.text.as_str()).or_default().push(b.center());
        out.runs.push(Run { layer: layer.index, text: text.clone(), x: b.x, y: b.y, down, at: b.center() });
    };
    match &named.shape {
        Shape::Pin(at) => {
            let (px, py) = units(*at);
            match p.marker {
                None => {
                    let b = level(px, py);
                    if fits(&b, taken) && far(&b, seen) {
                        keep(b, false, taken, seen, out);
                        return true;
                    }
                    false
                }
                Some(marker) => {
                    let side = match marker {
                        Marker::Dot(d) => d as i64,
                        Marker::Char(_) => 1,
                    }
                    .max(1);
                    let dot = Box { x: (px - side as f64 / 2.0).round() as i64, y: (py - side as f64 / 2.0).round() as i64, w: side, h: side };
                    if !fits(&dot, taken) {
                        return false;
                    }
                    let gap = if lw > 1 { (lw / 2).max(1) } else { 1 };
                    let mid = dot.y + side / 2 - across.1 / 2;
                    let mut spots = [
                        Box { x: dot.x + side + gap, y: mid, w: across.0, h: across.1 },
                        Box { x: dot.x - gap - across.0, y: mid, w: across.0, h: across.1 },
                        Box { x: dot.x + side / 2 - across.0 / 2, y: dot.y - across.1 - (gap / 2).max(if lw > 1 { 1 } else { 0 }), w: across.0, h: across.1 },
                        Box { x: dot.x + side / 2 - across.0 / 2, y: dot.y + side + (gap / 2).max(if lw > 1 { 1 } else { 0 }), w: across.0, h: across.1 },
                    ];
                    // The side it was on first.
                    let near = |b: &Box| was.iter().map(|&(x, y)| (b.center().0 - x).powi(2) + (b.center().1 - y).powi(2)).fold(f64::INFINITY, f64::min);
                    spots.sort_by(|a, b| near(a).total_cmp(&near(b)));
                    // The padding keeps other names off; the name's own
                    // mark sits inside it (one cell off, on text).
                    let Some(b) = spots.into_iter().find(|b| fits(b, taken) && !b.hits_box(&dot) && far(b, seen)) else {
                        return false;
                    };
                    dot.take(taken);
                    out.marks.push(Mark { layer: layer.index, x: dot.x, y: dot.y, marker });
                    keep(b, false, taken, seen, out);
                    true
                }
            }
        }
        Shape::Area(poly) => {
            let Some(inner) = poly.interior_point() else { return false };
            let (cx, cy) = units(inner.0);
            let b = level(cx, cy);
            // Every corner and the middle of every side inside the area.
            let to_page = |x: f64, y: f64| Point::new(x * m.unit.0, y * m.unit.1);
            let (x0, y0, x1, y1) = (b.x as f64, b.y as f64, (b.x + b.w) as f64, (b.y + b.h) as f64);
            let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let within = [(x0, y0), (xm, y0), (x1, y0), (x0, ym), (x1, ym), (x0, y1), (xm, y1), (x1, y1)]
                .iter()
                .all(|&(x, y)| poly.contains(&to_page(x, y)));
            if within && fits(&b, taken) && far(&b, seen) {
                keep(b, false, taken, seen, out);
                return true;
            }
            false
        }
        Shape::Road(pts) => along(pts, &text, was, only_was, layer, taken, seen, out, &mut keep, &fits, &far),
    }
}

impl Box {
    fn hits_box(&self, o: &Box) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

/// A road's name, level where the road runs across the screen and one
/// letter under the other where it runs down, wherever a stretch of it is
/// straight enough for the letters, `spacing` apart.
#[allow(clippy::too_many_arguments)]
fn along<'a>(
    pts: &[Coord<f64>],
    text: &[char],
    was: &[(f64, f64)],
    only_was: bool,
    layer: &Layer,
    taken: &mut Mask,
    seen: &mut HashMap<&'a str, Vec<(f64, f64)>>,
    out: &mut Placed,
    keep: &mut impl FnMut(Box, bool, &mut Mask, &mut HashMap<&'a str, Vec<(f64, f64)>>, &mut Placed),
    fits: &impl Fn(&Box, &Mask) -> bool,
    far: &impl Fn(&Box, &HashMap<&str, Vec<(f64, f64)>>) -> bool,
) -> bool {
    let m = layer.metrics;
    let n = text.len() as f64;
    let (lw, lh) = (m.letter.0 as f64, m.letter.1 as f64);
    // The letters' length in the output's units, level and down. Down, a cell holds one
    // letter a row; pixels turn the letters a quarter.
    let level_len = n * lw * m.unit.0;
    let cells = m.letter == (1, 1);
    let down_len = if cells { n * m.unit.1 } else { n * lw * m.unit.1 };
    let mut cum = vec![0.0];
    for w in pts.windows(2) {
        let d = ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
        cum.push(cum.last().unwrap() + d);
    }
    let length = *cum.last().unwrap();
    let at = |s: f64| -> Coord<f64> {
        let i = cum.partition_point(|&c| c <= s).clamp(1, pts.len() - 1);
        let (a, b) = (pts[i - 1], pts[i]);
        let seg = cum[i] - cum[i - 1];
        let t = if seg > 0.0 { ((s - cum[i - 1]) / seg).clamp(0.0, 1.0) } else { 0.0 };
        Coord { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }
    };
    let bow = |s0: f64, s1: f64| -> f64 {
        let (a, b) = (at(s0), at(s1));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        (0..pts.len())
            .filter(|&i| cum[i] > s0 && cum[i] < s1)
            .map(|i| ((pts[i].x - a.x) * dy - (pts[i].y - a.y) * dx).abs() / len)
            .fold(0.0, f64::max)
    };
    let tolerance = lh * m.unit.1 * 0.5;
    let spacing_len = layer.paint.spacing as f64 * m.unit.0;
    let step = (m.unit.0 * lw * 2.0).max(m.unit.0);
    let short = level_len.min(down_len);
    // The name centred `s` along the road, level or down, if a stretch
    // there is straight enough and it fits.
    // `across`: level across the road at `s`, however it runs.
    let mut put = |s: f64, across: bool, taken: &mut Mask, seen: &mut HashMap<&'a str, Vec<(f64, f64)>>, out: &mut Placed| -> bool {
        if across {
            let mid = at(s.clamp(0.0, length));
            let (cx, cy) = (mid.x / m.unit.0, mid.y / m.unit.1);
            let (w, h) = (text.len() as i64 * m.letter.0, m.letter.1);
            let bx = Box { x: (cx - w as f64 / 2.0).round() as i64, y: (cy - h as f64 / 2.0).round() as i64, w, h };
            let ok = fits(&bx, taken) && far(&bx, seen);
            if ok {
                keep(bx, false, taken, seen, out);
            }
            return ok;
        }
        for (want, down) in [(level_len, false), (down_len, true)] {
            if s < want / 2.0 || s > length - want / 2.0 {
                continue;
            }
            let (s0, s1) = (s - want / 2.0, s + want / 2.0);
            let (a, b) = (at(s0), at(s1));
            let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees().abs();
            let angle = if angle > 90.0 { 180.0 - angle } else { angle };
            let ok_angle = if down { angle >= 60.0 } else { angle <= 25.0 };
            if !ok_angle || bow(s0, s1) > tolerance {
                continue;
            }
            let mid = at(s);
            let (cx, cy) = (mid.x / m.unit.0, mid.y / m.unit.1);
            let (w, h) = match (down, cells) {
                (true, true) => (1, text.len() as i64),
                (true, false) => (m.letter.1, text.len() as i64 * m.letter.0),
                (false, _) => (text.len() as i64 * m.letter.0, m.letter.1),
            };
            let bx = Box { x: (cx - w as f64 / 2.0).round() as i64, y: (cy - h as f64 / 2.0).round() as i64, w, h };
            if fits(&bx, taken) && far(&bx, seen) {
                keep(bx, down, taken, seen, out);
                return true;
            }
        }
        false
    };
    let mut any = false;
    // Where the name was, if the road still passes there: the nearest
    // point of it to each, a letter or two off at most.
    for &(x, y) in was {
        let (px, py) = (x * m.unit.0, y * m.unit.1);
        let mut best: Option<(f64, f64)> = None;
        for (i, w) in pts.windows(2).enumerate() {
            let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 { (((px - w[0].x) * dx + (py - w[0].y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let d = (w[0].x + dx * t - px).powi(2) + (w[0].y + dy * t - py).powi(2);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, cum[i] + t * len2.sqrt()));
            }
        }
        if let Some((d, s)) = best {
            if d.sqrt() <= 2.0 * lh * m.unit.1 {
                // Along the road there, or else level across it, as a road
                // with no straight stretch is named.
                let ok = put(s, false, taken, seen, out) || (length >= level_len && put(s, true, taken, seen, out));
                any |= ok;
            }
        }
    }
    if only_was {
        return any;
    }
    let mut s = short / 2.0;
    let mut tries = 0;
    while s <= length - short / 2.0 && tries < 400 {
        tries += 1;
        let placed = put(s, false, taken, seen, out);
        any |= placed;
        s += if placed { spacing_len.max(step) } else { step };
    }
    if any || length < level_len {
        return any;
    }
    // No straight stretch held the name: level, across the road (one at
    // least as long as the name), on the first point of it with room.
    let mut s = length / 2.0;
    let mut k = 0;
    while k < 40 {
        if put(s, true, taken, seen, out) {
            return true;
        }
        k += 1;
        // Out from the middle: +1, -1, +2, -2 steps…
        let off = ((k + 1) / 2) as f64 * step * if k % 2 == 1 { 1.0 } else { -1.0 };
        if off.abs() > length / 2.0 {
            break;
        }
        s = length / 2.0 + off;
    }
    false
}

/// Where the `i`th letter of a run sits, in grid units: along the run, or
/// down it (a row a letter in cells; a letter's width each in pixels, the
/// letters turned a quarter).
pub fn letter_at(run: &Run, i: usize, letter: (i64, i64), cells: bool) -> (i64, i64) {
    let i = i as i64;
    match (run.down, cells) {
        (false, _) => (run.x + i * letter.0, run.y),
        (true, true) => (run.x, run.y + i),
        (true, false) => (run.x, run.y + i * letter.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drawland_core::labels::Named;

    fn layer<'a>(paint: &'a TextPaint, names: &'a [Named]) -> Layer<'a> {
        Layer { index: 0, id: "", paint, names, metrics: Metrics { letter: (1, 1), unit: (1.0, 2.0), size: (40, 20) } }
    }

    fn road(text: &str, pts: &[(f64, f64)]) -> Named {
        Named { text: text.into(), rank: 0.5, extent: 1.0, shape: Shape::Road(pts.iter().map(|&(x, y)| Coord { x, y }).collect()), feature: 0 }
    }

    #[test]
    fn a_level_road_is_named_along_it_and_a_steep_one_down_it() {
        let paint = TextPaint { spacing: 100, ..TextPaint::default() };
        let names = vec![road("Main", &[(2.0, 10.0), (30.0, 10.0)]), road("Elm", &[(36.0, 2.0), (36.0, 38.0)])];
        let placed = place(&[layer(&paint, &names)], (40, 20), &Hints::new());
        assert_eq!(placed.runs.len(), 2);
        let main = &placed.runs[0];
        assert!(!main.down && main.y == 5 && main.text.len() == 4);
        let elm = &placed.runs[1];
        assert!(elm.down && elm.x == 36);
    }

    #[test]
    fn a_name_stays_where_it_was() {
        let paint = TextPaint { spacing: 100, ..TextPaint::default() };
        let names = vec![road("Main", &[(2.0, 10.0), (38.0, 10.0)])];
        let first = place(&[layer(&paint, &names)], (40, 20), &Hints::new()).runs[0].x;
        // Last time it was at x = 30: the road passes there, so there it goes.
        let hints = Hints::from([((String::new(), "Main".to_string()), vec![(30.0, 5.4)])]);
        let run = &place(&[layer(&paint, &names)], (40, 20), &hints).runs[0];
        assert_ne!(first, 28);
        assert_eq!((run.x, run.y), (28, 5));
        // A hint far off the road is no hint.
        let hints = Hints::from([((String::new(), "Main".to_string()), vec![(30.0, 15.0)])]);
        assert_eq!(place(&[layer(&paint, &names)], (40, 20), &hints).runs[0].x, first);
    }

    #[test]
    fn names_never_overlap() {
        let paint = TextPaint { spacing: 30, padding: 0, ..TextPaint::default() };
        let names = vec![road("Alpha", &[(0.0, 10.0), (40.0, 10.0)]), road("Beta", &[(0.0, 10.4), (40.0, 10.4)])];
        let placed = place(&[layer(&paint, &names)], (40, 20), &Hints::new());
        let mut cells = std::collections::HashSet::new();
        for r in &placed.runs {
            for i in 0..r.text.len() {
                assert!(cells.insert(letter_at(r, i, (1, 1), true)));
            }
        }
        assert!(placed.runs.iter().any(|r| r.text[0] == 'B'));
    }

    #[test]
    fn a_marked_place_has_its_name_beside_the_mark() {
        let paint = TextPaint { marker: Some(Marker::Char('*')), ..TextPaint::default() };
        let names = vec![Named { text: "Pavia".into(), rank: 1.0, extent: 1.0, shape: Shape::Pin(Coord { x: 10.5, y: 21.0 }), feature: 0 }];
        let placed = place(&[layer(&paint, &names)], (40, 20), &Hints::new());
        assert_eq!(placed.marks.len(), 1);
        let (mark, run) = (&placed.marks[0], &placed.runs[0]);
        assert_eq!((mark.x, mark.y), (10, 10));
        assert_eq!((run.x, run.y), (12, 10));
        // Padded, as the terminal profile's places are: the padding keeps
        // other names off, not the name's own mark a cell away.
        let padded = TextPaint { padding: 2, ..paint };
        let placed = place(&[layer(&padded, &names)], (40, 20), &Hints::new());
        assert_eq!((placed.marks.len(), placed.runs.len()), (1, 1));
        assert_eq!((placed.runs[0].x, placed.runs[0].y), (12, 10));
    }
}
