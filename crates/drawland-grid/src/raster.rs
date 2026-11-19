//! Masks: which dots of a grid something covers. Areas are scanned at the
//! dots' centres (even-odd), lines stepped dot by dot with no anti-aliasing,
//! and a mask grown by a halo is the clear space round it.

use geo::{Coord, LineString, MultiPolygon};

/// One bit per dot, `w` across and `h` down.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    bits: Vec<bool>,
}

impl Mask {
    pub fn new(w: usize, h: usize) -> Mask {
        Mask { w, h, bits: vec![false; w * h] }
    }

    pub fn get(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h && self.bits[y as usize * self.w + x as usize]
    }

    pub fn set(&mut self, x: i64, y: i64) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.bits[y as usize * self.w + x as usize] = true;
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.bits.iter().any(|b| *b)
    }

    /// Every dot set, row by row.
    pub fn dots(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.bits.iter().enumerate().filter(|(_, b)| **b).map(move |(i, _)| (i % self.w, i / self.w))
    }

    /// Every dot within `rx` across and `ry` down of a set one: an ellipse
    /// when `round`, else a rectangle.
    pub fn grown(&self, rx: u32, ry: u32, round: bool) -> Mask {
        if rx == 0 && ry == 0 {
            return self.clone();
        }
        let (rx, ry) = (rx as i64, ry as i64);
        if !round {
            // A rectangle is a row pass, then a column pass.
            let mut across = Mask::new(self.w, self.h);
            for (x, y) in self.dots() {
                for dx in -rx..=rx {
                    across.set(x as i64 + dx, y as i64);
                }
            }
            let mut out = Mask::new(self.w, self.h);
            for (x, y) in across.dots() {
                for dy in -ry..=ry {
                    out.set(x as i64, y as i64 + dy);
                }
            }
            return out;
        }
        let mut offsets = Vec::new();
        for dy in -ry..=ry {
            for dx in -rx..=rx {
                let fx = if rx == 0 { 0.0 } else { dx as f64 / (rx as f64 + 0.5) };
                let fy = if ry == 0 { 0.0 } else { dy as f64 / (ry as f64 + 0.5) };
                if fx * fx + fy * fy <= 1.0 {
                    offsets.push((dx, dy));
                }
            }
        }
        let mut out = Mask::new(self.w, self.h);
        for (x, y) in self.dots() {
            let (x, y) = (x as i64, y as i64);
            for &(dx, dy) in &offsets {
                out.set(x + dx, y + dy);
            }
        }
        out
    }

    pub fn union(&mut self, other: &Mask) {
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a |= *b;
        }
    }
}

/// From output millimetres to dots: `x / k`, `y / k`.
#[derive(Clone, Copy, Debug)]
pub struct ToDots {
    pub mm_per_dot: f64,
}

impl ToDots {
    pub fn at(&self, c: Coord<f64>) -> (f64, f64) {
        (c.x / self.mm_per_dot, c.y / self.mm_per_dot)
    }
}

/// The dots of a `w` × `h` grid whose centres lie inside `area` (even-odd).
pub fn area_mask(area: &MultiPolygon<f64>, to: ToDots, w: usize, h: usize) -> Mask {
    let mut mask = Mask::new(w, h);
    let mut rows: Vec<Vec<f64>> = vec![Vec::new(); h];
    let rings = area.0.iter().flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors()));
    for ring in rings {
        let pts: Vec<(f64, f64)> = ring.0.iter().map(|&c| to.at(c)).collect();
        for pair in pts.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            if y0 == y1 {
                continue;
            }
            let (lo, hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
            // Rows whose centre y + 0.5 is in [lo, hi).
            let first = (lo - 0.5).ceil().max(0.0) as i64;
            let last = ((hi - 0.5).ceil() as i64 - 1).min(h as i64 - 1);
            for row in first..=last {
                let yc = row as f64 + 0.5;
                let t = (yc - y0) / (y1 - y0);
                rows[row as usize].push(x0 + t * (x1 - x0));
            }
        }
    }
    for (row, xs) in rows.iter_mut().enumerate() {
        xs.sort_by(f64::total_cmp);
        for span in xs.chunks_exact(2) {
            let a = (span[0] - 0.5).ceil().max(0.0) as i64;
            let b = ((span[1] - 0.5).ceil() as i64).min(w as i64);
            for x in a..b {
                mask.set(x, row as i64);
            }
        }
    }
    mask
}

/// `(x0, y0)`–`(x1, y1)` cut to the box `[lo, hi]` (Liang–Barsky), or `None`
/// when it misses it.
pub fn clip(a: (f64, f64), b: (f64, f64), lo: (f64, f64), hi: (f64, f64)) -> Option<((f64, f64), (f64, f64))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.0 - lo.0), (dx, hi.0 - a.0), (-dy, a.1 - lo.1), (dy, hi.1 - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            t0 = t0.max(r);
        } else {
            if r < t0 {
                return None;
            }
            t1 = t1.min(r);
        }
    }
    Some(((a.0 + t0 * dx, a.1 + t0 * dy), (a.0 + t1 * dx, a.1 + t1 * dy)))
}

/// The dots a line steps through, in order, each touching the one before
/// at a side or a corner (Bresenham), from its own start to its end. A
/// line is cut to a margin round the grid first, so one far off it costs
/// nothing. Consecutive points within a line share their dot.
pub fn line_dots(line: &LineString<f64>, to: ToDots, w: usize, h: usize) -> Vec<Vec<(i64, i64)>> {
    let margin = 2.0;
    let lo = (-margin, -margin);
    let hi = (w as f64 + margin, h as f64 + margin);
    let mut runs: Vec<Vec<(i64, i64)>> = Vec::new();
    let mut run: Vec<(i64, i64)> = Vec::new();
    let mut joined_at: Option<(f64, f64)> = None;
    for pair in line.0.windows(2) {
        let (a, b) = (to.at(pair[0]), to.at(pair[1]));
        let Some((a, b)) = clip(a, b, lo, hi) else {
            if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
            joined_at = None;
            continue;
        };
        if joined_at.is_none_or(|j| (j.0 - a.0).abs() > 1e-9 || (j.1 - a.1).abs() > 1e-9) && !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
        let (mut x, mut y) = (a.0.floor() as i64, a.1.floor() as i64);
        let (x1, y1) = (b.0.floor() as i64, b.1.floor() as i64);
        let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
        let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            if run.last() != Some(&(x, y)) {
                run.push((x, y));
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
        joined_at = Some(b);
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// The cells a segment crosses, each sharing a side with the one before
/// (Amanatides–Woo): what box-drawing characters join along. Coordinates
/// are in cells.
pub fn cell_walk(a: (f64, f64), b: (f64, f64), mut visit: impl FnMut(i64, i64)) {
    let (mut x, mut y) = (a.0.floor() as i64, a.1.floor() as i64);
    let (x1, y1) = (b.0.floor() as i64, b.1.floor() as i64);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let step_x = if dx > 0.0 { 1 } else { -1 };
    let step_y = if dy > 0.0 { 1 } else { -1 };
    let t_delta_x = if dx != 0.0 { (1.0 / dx).abs() } else { f64::INFINITY };
    let t_delta_y = if dy != 0.0 { (1.0 / dy).abs() } else { f64::INFINITY };
    let frac = |v: f64, step: i64| if step > 0 { v.floor() + 1.0 - v } else { v - v.floor() };
    let mut t_max_x = if dx != 0.0 { frac(a.0, step_x) * t_delta_x } else { f64::INFINITY };
    let mut t_max_y = if dy != 0.0 { frac(a.1, step_y) * t_delta_y } else { f64::INFINITY };
    visit(x, y);
    let limit = (x1 - x).abs() + (y1 - y).abs();
    for _ in 0..limit {
        if t_max_x < t_max_y {
            x += step_x;
            t_max_x += t_delta_x;
        } else {
            y += step_y;
            t_max_y += t_delta_y;
        }
        visit(x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{polygon, MultiPolygon};

    #[test]
    fn a_square_covers_the_dots_whose_centres_it_holds() {
        let sq = MultiPolygon::new(vec![polygon![(x: 1.0, y: 1.0), (x: 4.0, y: 1.0), (x: 4.0, y: 3.0), (x: 1.0, y: 3.0)]]);
        let m = area_mask(&sq, ToDots { mm_per_dot: 1.0 }, 6, 5);
        let got: Vec<(usize, usize)> = m.dots().collect();
        assert_eq!(got, vec![(1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (3, 2)]);
    }

    #[test]
    fn a_line_steps_dot_by_dot_and_stays_on_the_grid_margin() {
        let l = LineString::from(vec![(0.5, 0.5), (4.5, 2.5), (1000.0, 2.5)]);
        let runs = line_dots(&l, ToDots { mm_per_dot: 1.0 }, 8, 4);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0][0], (0, 0));
        assert!(runs[0].windows(2).all(|p| (p[0].0 - p[1].0).abs() <= 1 && (p[0].1 - p[1].1).abs() <= 1));
        assert!(runs[0].iter().all(|&(x, _)| x <= 10));
    }

    #[test]
    fn a_cell_walk_shares_sides() {
        let mut cells = Vec::new();
        cell_walk((0.5, 0.5), (3.5, 2.5), |x, y| cells.push((x, y)));
        assert_eq!(cells.first(), Some(&(0, 0)));
        assert_eq!(cells.last(), Some(&(3, 2)));
        assert!(cells.windows(2).all(|p| (p[0].0 - p[1].0).abs() + (p[0].1 - p[1].1).abs() == 1));
    }

    #[test]
    fn a_halo_grows_round() {
        let mut m = Mask::new(9, 9);
        m.set(4, 4);
        let g = m.grown(2, 2, true);
        assert!(g.get(4, 2) && g.get(6, 4) && g.get(5, 5));
        assert!(!g.get(6, 6));
        let r = m.grown(2, 1, false);
        assert!(r.get(6, 5) && !r.get(4, 6));
    }
}
