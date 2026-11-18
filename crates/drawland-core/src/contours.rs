//! Features made from the elevation: contour lines and soundings. The heights
//! become tags, as an OSM feature's do, so a script colours, filters and labels
//! them with the tools it already has (`features.where`, `ink.ramp`,
//! `modules.labels`) without ever reading a number.
//!
//! Every feature carries:
//! - `ele`: its height in metres (negative under the sea);
//! - `depth`: metres below the surface, under the sea only;
//! - `t`: where it sits between the lowest and the highest feature of the
//!   set on this map, 0 to 1 — a colour ramp that fits any place;
//! - contours only, `index`: on every `index`th step (the labelled ones).
//!
//! Contours are traced from the heights smoothed as `areas.band` smooths them
//! (pixel centres at +0.5, as burin's), so a contour runs along the edge of the
//! band that starts at its height.

use crate::geom::{self, Lines};
use crate::osm::{Element, Feature, Tags};
use crate::terrain::{Grid, Placed};
use geo::{Coord, Geometry, LineString, MultiPolygon, Point};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// `terrain_select`: which features to make from the elevation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TerrainSelect {
    Contours(ContourSpec),
    Soundings(SoundingSpec),
}

/// Contour lines every `step` metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContourSpec {
    /// Metres between contours.
    pub step: f64,
    /// The lowest and highest contour drawn; unset, what the map reaches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<f64>,
    /// Every `index`th contour is an index contour (tagged `index`).
    #[serde(default = "default_index")]
    pub index: u32,
    /// Smoothing of the heights, grid px (as `areas.band`'s `blur`).
    #[serde(default = "default_contour_blur")]
    pub blur: f64,
    /// Pieces shorter than this, in mm, are dropped: specks and pits.
    #[serde(default = "default_min_length")]
    pub min_length: f64,
}

/// A height or depth read at points `every` mm apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundingSpec {
    pub every: f64,
    /// Where to keep points: the sea (soundings), the land (spot heights), or both.
    #[serde(default)]
    pub on: On,
    /// Smoothing of the heights, grid px.
    #[serde(default = "default_sounding_blur")]
    pub blur: f64,
    /// How far each point strays from its place on the lattice, 0 (a
    /// lattice) to 1 (up to half the distance between points): a chart's
    /// soundings look read by hand. Deterministic: the same map, the same points.
    #[serde(default)]
    pub jitter: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum On {
    #[default]
    Sea,
    Land,
    All,
}

fn default_index() -> u32 {
    5
}
fn default_contour_blur() -> f64 {
    4.0
}
fn default_sounding_blur() -> f64 {
    2.0
}
fn default_min_length() -> f64 {
    3.0
}

impl TerrainSelect {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            TerrainSelect::Contours(c) => {
                if !(c.step.is_finite() && c.step > 0.0) {
                    return Err(format!("contours: step is a positive number of metres, got {}", c.step));
                }
                if let (Some(a), Some(b)) = (c.from, c.to) {
                    if a > b {
                        return Err(format!("contours: from ({a}) is above to ({b})"));
                    }
                }
                if c.index == 0 {
                    return Err("contours: index is a whole number of steps, 1 or more".into());
                }
                if !(c.blur >= 0.0 && c.min_length >= 0.0) {
                    return Err("contours: blur and minLength are 0 or more".into());
                }
            }
            TerrainSelect::Soundings(s) => {
                if !(s.every.is_finite() && s.every >= 0.5) {
                    return Err(format!("soundings: every is a length of 0.5 mm or more, got {}", s.every));
                }
                if s.blur < 0.0 {
                    return Err("soundings: blur is 0 or more".into());
                }
                if !(0.0..=1.0).contains(&s.jitter) {
                    return Err(format!("soundings: jitter is 0 to 1, got {}", s.jitter));
                }
            }
        }
        Ok(())
    }
}

/// The features `spec` makes from `grid`, in output mm, inside `frame`.
pub fn features(spec: &TerrainSelect, grid: &Grid, placed: &Placed, frame: &MultiPolygon<f64>) -> Vec<Feature> {
    let mut out = match spec {
        TerrainSelect::Contours(c) => contours(c, grid, placed, frame),
        TerrainSelect::Soundings(s) => soundings(s, grid, placed, frame),
    };
    // `t`: 0 at the lowest feature on this map, 1 at the highest.
    let eles: Vec<f64> = out.iter().filter_map(|f| f.tag("ele")?.parse().ok()).collect();
    let (lo, hi) = eles.iter().fold((f64::MAX, f64::MIN), |(a, b), &e| (a.min(e), b.max(e)));
    for (f, e) in out.iter_mut().zip(eles) {
        let t = if hi > lo { (e - lo) / (hi - lo) } else { 0.0 };
        f.tags.insert("t".into(), format!("{:.4}", t));
    }
    out
}

/// A height as a tag: whole metres when it is whole, else to the centimetre.
fn metres(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }
}

fn height_tags(ele: f64) -> Tags {
    let mut tags = Tags::new();
    tags.insert("ele".into(), metres(ele));
    if ele < 0.0 {
        tags.insert("depth".into(), metres(-ele));
    }
    tags
}

fn contours(c: &ContourSpec, grid: &Grid, placed: &Placed, frame: &MultiPolygon<f64>) -> Vec<Feature> {
    let smooth = crate::terrain::smoothed(grid, c.blur);
    let levels = trace_levels(&smooth.elev, smooth.width, smooth.height, c.step, c.from, c.to);
    let mut out = Vec::new();
    let mut id = 0;
    for (k, chains) in levels {
        let ele = k as f64 * c.step;
        let lines: Lines = chains
            .into_iter()
            .map(|chain| {
                LineString::new(
                    chain
                        .into_iter()
                        .map(|[x, y]| {
                            let (x, y) = placed.to_mm(x, y);
                            Coord { x, y }
                        })
                        .collect(),
                )
            })
            .collect();
        let mut tags = height_tags(ele);
        if k.rem_euclid(c.index as i64) == 0 {
            tags.insert("index".into(), "yes".into());
        }
        for piece in geom::clip_lines(&lines, frame, false) {
            if geo::Length::length(&geo::Euclidean, &piece) < c.min_length {
                continue;
            }
            id += 1;
            out.push(Feature { element: Element::Way, id, tags: tags.clone(), geometry: Geometry::LineString(piece) });
        }
    }
    out
}

/// Every level `k · step` within `from..=to` and the heights, traced across
/// `elev` (`w × h`, samples at pixel centres): per level, the chains in grid
/// px. One pass over the cells for all levels.
fn trace_levels(elev: &[f32], w: usize, h: usize, step: f64, from: Option<f64>, to: Option<f64>) -> Vec<(i64, Vec<Vec<[f64; 2]>>)> {
    if w < 2 || h < 2 {
        return Vec::new();
    }
    let k_lo = from.map(|f| (f / step).ceil() as i64).unwrap_or(i64::MIN);
    let k_hi = to.map(|t| (t / step).floor() as i64).unwrap_or(i64::MAX);
    // Edges by id: 2·i joins sample i to the one right of it, 2·i + 1 to the
    // one below. Each crossed edge links to the crossed edges of the cells on
    // either side: open chains end at the grid's edge, the rest close.
    let mut links: HashMap<i64, HashMap<usize, [usize; 2]>> = HashMap::new();
    let join = |m: &mut HashMap<usize, [usize; 2]>, a: usize, b: usize| {
        for (from, to) in [(a, b), (b, a)] {
            let l = m.entry(from).or_insert([usize::MAX; 2]);
            l[(l[0] != usize::MAX) as usize] = to;
        }
    };
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let i = y * w + x;
            let corners = [i, i + 1, i + 1 + w, i + w];
            let v = corners.map(|c| elev[c] as f64);
            let (lo, hi) = v.iter().fold((f64::MAX, f64::MIN), |(a, b), &e| (a.min(e), b.max(e)));
            // The levels this cell crosses: `level` strictly inside (lo, hi]
            // as `v > level` reads inside.
            let first = ((lo / step).floor() as i64 + 0).max(k_lo);
            let last = ((hi / step).ceil() as i64).min(k_hi);
            if first > last {
                continue;
            }
            let edges = [2 * i, 2 * (i + 1) + 1, 2 * (i + w), 2 * i + 1];
            for k in first..=last {
                let level = k as f64 * step;
                let s = v.map(|e| e > level);
                let crossed: Vec<usize> = (0..4).filter(|&e| s[e] != s[(e + 1) % 4]).collect();
                match crossed[..] {
                    [a, b] => join(links.entry(k).or_default(), edges[a], edges[b]),
                    [_, _, _, _] => {
                        let centre = v.iter().sum::<f64>() / 4.0 > level;
                        let m = links.entry(k).or_default();
                        for e in (0..4).filter(|&e| s[e] != centre) {
                            join(m, edges[(e + 3) % 4], edges[e]);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let point = |id: usize, level: f64| -> [f64; 2] {
        let (i, across) = (id / 2, id % 2 == 0);
        let j = if across { i + 1 } else { i + w };
        let (a, b) = (elev[i] as f64 - level, elev[j] as f64 - level);
        let t = if a == b { 0.5 } else { a / (a - b) };
        let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        if across { [x + t, y] } else { [x, y + t] }
    };
    let mut out: Vec<(i64, Vec<Vec<[f64; 2]>>)> = links
        .into_iter()
        .map(|(k, m)| {
            let level = k as f64 * step;
            let mut done = std::collections::HashSet::new();
            let mut chains = Vec::new();
            // Open chains first, from their ends; then the closed loops.
            let mut starts: Vec<usize> = m.keys().copied().collect();
            starts.sort_unstable_by_key(|s| (m[s][1] != usize::MAX, *s));
            for start in starts {
                if !done.insert(start) {
                    continue;
                }
                let mut chain = vec![point(start, level)];
                let (mut prev, mut cur) = (start, m[&start][0]);
                while cur != usize::MAX && done.insert(cur) {
                    chain.push(point(cur, level));
                    let [a, b] = m[&cur];
                    (prev, cur) = (cur, if a == prev { b } else { a });
                }
                if cur == start {
                    chain.push(chain[0]);
                }
                if chain.len() >= 2 {
                    chains.push(chain);
                }
            }
            (k, chains)
        })
        .collect();
    out.sort_by_key(|(k, _)| *k);
    out
}

fn soundings(s: &SoundingSpec, grid: &Grid, placed: &Placed, frame: &MultiPolygon<f64>) -> Vec<Feature> {
    let smooth = crate::terrain::smoothed(grid, s.blur);
    let Some(rect) = geo::BoundingRect::bounding_rect(frame) else { return Vec::new() };
    let (w, h) = (smooth.width, smooth.height);
    // The height at a point, between the four nearest samples.
    let at = |x: f64, y: f64| -> Option<f64> {
        let [px, py] = placed.to_px(x, y);
        let (sx, sy) = (px - 0.5, py - 0.5);
        if sx < 0.0 || sy < 0.0 || sx > (w - 1) as f64 || sy > (h - 1) as f64 {
            return None;
        }
        let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (sx - x0 as f64, sy - y0 as f64);
        let e = |x: usize, y: usize| smooth.elev[y * w + x] as f64;
        let top = e(x0, y0) * (1.0 - fx) + e(x1, y0) * fx;
        let bottom = e(x0, y1) * (1.0 - fx) + e(x1, y1) * fx;
        Some(top * (1.0 - fy) + bottom * fy)
    };
    // A staggered lattice: each row half a step along, rows √3/2 apart.
    let row = s.every * 3f64.sqrt() / 2.0;
    let mut out = Vec::new();
    let mut r = 0u64;
    let mut y0 = rect.min().y + row / 2.0;
    while y0 < rect.max().y {
        let mut c = 0u64;
        let mut x0 = rect.min().x + s.every * if r % 2 == 0 { 0.5 } else { 1.0 };
        while x0 < rect.max().x {
            // Each point moved by its own fixed pseudo-random offset.
            let reach = s.jitter * s.every / 2.0;
            let (x, y) = (x0 + reach * (unit(r, c, 1) * 2.0 - 1.0), y0 + reach * (unit(r, c, 2) * 2.0 - 1.0));
            c += 1;
            if geo::Contains::contains(frame, &Point::new(x, y)) {
                if let Some(ele) = at(x, y) {
                    // The sea as burin draws it: at or below half a metre.
                    let keep = match s.on {
                        On::Sea => ele <= 0.5 && ele < 0.0,
                        On::Land => ele > 0.5,
                        On::All => true,
                    };
                    if keep {
                        let ele = ele.round();
                        out.push(Feature {
                            element: Element::Node,
                            id: out.len() as i64 + 1,
                            tags: height_tags(ele),
                            geometry: Geometry::Point(Point::new(x, y)),
                        });
                    }
                }
            }
            x0 += s.every;
        }
        y0 += row;
        r += 1;
    }
    out
}

/// A fixed number in [0, 1) for lattice point `(r, c)` and channel `k`.
fn unit(r: u64, c: u64, k: u64) -> f64 {
    let mut z = r.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ c.wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ k.wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cone: `peak` metres at the middle, falling 1 m per px.
    fn cone(n: usize, peak: f32) -> Vec<f32> {
        let c = (n as f32 - 1.0) / 2.0;
        (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f32, (i / n) as f32);
                peak - ((x - c).hypot(y - c))
            })
            .collect()
    }

    #[test]
    fn a_cone_gives_one_closed_ring_per_level() {
        let n = 41;
        let levels = trace_levels(&cone(n, 15.0), n, n, 5.0, None, None);
        let ks: Vec<i64> = levels.iter().map(|(k, _)| *k).collect();
        // 15 m at the top: levels 10, 5 and 0 are circles inside the grid
        // (radius 5, 10, 15 px); lower ones run off its edges.
        for k in [0, 1, 2] {
            let (_, chains) = levels.iter().find(|(kk, _)| *kk == k).unwrap_or_else(|| panic!("level {k} in {ks:?}"));
            assert_eq!(chains.len(), 1, "level {k}");
            let ring = &chains[0];
            assert_eq!(ring.first(), ring.last(), "level {k} closes");
            // Radius 15 - level px from the centre (20.5 in px with the +0.5).
            let r = 15.0 - k as f64 * 5.0;
            for p in ring {
                let d = (p[0] - 20.5).hypot(p[1] - 20.5);
                assert!((d - r).abs() < 0.6, "level {k}: {d} vs {r}");
            }
        }
    }

    #[test]
    fn from_and_to_keep_to_the_levels_asked() {
        let n = 41;
        let levels = trace_levels(&cone(n, 15.0), n, n, 5.0, Some(0.0), Some(5.0));
        assert_eq!(levels.iter().map(|(k, _)| *k).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn a_slope_cut_by_the_edge_gives_open_lines() {
        // Heights rising west to east: every level a north–south line, open.
        let (w, h) = (30, 10);
        let elev: Vec<f32> = (0..w * h).map(|i| (i % w) as f32).collect();
        let levels = trace_levels(&elev, w, h, 10.0, None, None);
        let (_, chains) = levels.iter().find(|(k, _)| *k == 1).expect("level 10");
        assert_eq!(chains.len(), 1);
        assert_ne!(chains[0].first(), chains[0].last());
        assert!(chains[0].iter().all(|p| (p[0] - 10.5).abs() < 1e-9), "{:?}", chains[0]);
    }

    #[test]
    fn heights_read_as_tags() {
        assert_eq!(metres(150.0), "150");
        assert_eq!(metres(-12.5), "-12.5");
        let t = height_tags(-40.0);
        assert_eq!((t["ele"].as_str(), t["depth"].as_str()), ("-40", "40"));
        assert!(!height_tags(10.0).contains_key("depth"));
    }
}

#[cfg(test)]
mod heights {
    use super::*;

    /// Bilinear height at grid px `(x, y)`, samples at centres (+0.5).
    fn at(e: &[f32], w: usize, x: f64, y: f64) -> f64 {
        let (sx, sy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (sx.floor().max(0.0) as usize, sy.floor().max(0.0) as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(w - 1));
        let (fx, fy) = (sx - x0 as f64, sy - y0 as f64);
        let v = |x: usize, y: usize| e[y * w + x] as f64;
        (v(x0, y0) * (1.0 - fx) + v(x1, y0) * fx) * (1.0 - fy) + (v(x0, y1) * (1.0 - fx) + v(x1, y1) * fx) * fy
    }

    #[test]
    fn every_contour_point_lies_at_its_height() {
        let w = 96;
        let e: Vec<f32> = (0..w * w)
            .map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                200.0 + 150.0 * (x / 9.0).sin() * (y / 7.0).cos() + 60.0 * (x / 3.0 + y / 4.0).sin()
            })
            .collect();
        for (k, chains) in trace_levels(&e, w, w, 50.0, None, None) {
            for p in chains.iter().flatten() {
                let h = at(&e, w, p[0], p[1]);
                assert!((h - k as f64 * 50.0).abs() < 1e-3, "level {}: a point at {h}", k * 50);
            }
        }
    }
}
