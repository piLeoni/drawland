//! The orthographic projection: the Earth as a globe seen from far away
//! (`+proj=ortho +lat_0={lat} +lon_0={lng}`), on the sphere of web
//! Mercator's radius. proj4rs has no `ortho`, so it is here.
//!
//! Half the globe is out of sight. Geometry is cut at the horizon in lon/lat
//! before it is projected ([`Ortho::visible`]): on the unit sphere turned so
//! the view's centre is its top, the hidden half is `z < 0`; a line is cut
//! where it crosses the horizon, an area's ring is cut there too and closed
//! along the horizon, so the sea of the globe's rim ends on the rim. Long
//! edges (a world tile's) are first split to a degree, so they bend round
//! the globe as the ground does.

use geo::{Coord, Geometry, LineString, MultiLineString, MultiPolygon, Polygon};

pub const RADIUS: f64 = 6_378_137.0;

/// Points on the horizon this far inside it (on the unit sphere), so they
/// project in sight.
const EPS: f64 = 1e-7;

/// The longest edge, in degrees, left straight.
const STEP: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ortho {
    lon0: f64,
    sin0: f64,
    cos0: f64,
}

/// A point on the unit sphere turned to the view: `x` east and `y` north at
/// the centre, `z` toward the eye.
type V3 = (f64, f64, f64);

impl Ortho {
    /// The view from above `(lon0, lat0)`.
    pub fn new(lon0: f64, lat0: f64) -> Ortho {
        let (sin0, cos0) = lat0.to_radians().sin_cos();
        Ortho { lon0, sin0, cos0 }
    }

    /// An `ortho` PROJ string (its `{lat}` / `{lng}` already written in):
    /// its centre, `lat_0` and `lon_0`, 0 where left out. `None` for any
    /// other projection.
    pub fn parse(definition: &str) -> Option<Ortho> {
        let words: Vec<&str> = definition.split_whitespace().collect();
        if !words.contains(&"+proj=ortho") {
            return None;
        }
        let value = |key: &str| words.iter().find_map(|w| w.strip_prefix(key)).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        Some(Ortho::new(value("+lon_0="), value("+lat_0=")))
    }

    fn turn(&self, lon: f64, lat: f64) -> V3 {
        let (sp, cp) = lat.to_radians().sin_cos();
        let (sl, cl) = (lon - self.lon0).to_radians().sin_cos();
        (cp * sl, self.cos0 * sp - self.sin0 * cp * cl, self.sin0 * sp + self.cos0 * cp * cl)
    }

    fn back(&self, (x, y, z): V3) -> (f64, f64) {
        let lat = (y * self.cos0 + z * self.sin0).clamp(-1.0, 1.0).asin();
        let lon = x.atan2(z * self.cos0 - y * self.sin0);
        (wrap(self.lon0 + lon.to_degrees()), lat.to_degrees())
    }

    /// Projected metres; NaN on the far side.
    pub fn forward(&self, lon: f64, lat: f64) -> (f64, f64) {
        let (x, y, z) = self.turn(lon, lat);
        if z < -EPS {
            return (f64::NAN, f64::NAN);
        }
        (RADIUS * x, RADIUS * y)
    }

    /// `(lon, lat)` under projected metres; `None` off the globe.
    pub fn inverse(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let (x, y) = (x / RADIUS, y / RADIUS);
        let r2 = x * x + y * y;
        if r2 > 1.0 + 1e-12 {
            return None;
        }
        Some(self.back((x, y, (1.0 - r2).max(0.0).sqrt())))
    }

    /// The ground within `radius` degrees of the centre that a globe shows,
    /// as a lon/lat box: past a pole, every longitude to that pole.
    pub fn ground(&self, radius: f64) -> geo::Polygon<f64> {
        let r = radius.clamp(0.0, 90.0);
        let lat0 = self.sin0.atan2(self.cos0).to_degrees();
        let (mut south, mut north) = (lat0 - r, lat0 + r);
        let (mut west, mut east) = (-180.0, 180.0);
        if north >= 90.0 {
            north = 90.0;
        } else if south <= -90.0 {
            south = -90.0;
        } else {
            let s = r.to_radians().sin() / lat0.to_radians().cos();
            if s < 1.0 {
                let d = s.asin().to_degrees();
                if self.lon0 - d >= -180.0 && self.lon0 + d <= 180.0 {
                    (west, east) = (self.lon0 - d, self.lon0 + d);
                }
            }
        }
        geo::Polygon::new(LineString::from(vec![(west, south), (east, south), (east, north), (west, north), (west, south)]), vec![])
    }

    /// Degrees from the centre to `(lon, lat)`.
    pub fn distance(&self, lon: f64, lat: f64) -> f64 {
        self.turn(lon, lat).2.clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// The geometry's part in sight, in lon/lat.
    pub fn visible(&self, g: &Geometry<f64>) -> Geometry<f64> {
        match g {
            Geometry::Point(p) => {
                // Out of sight, a point is left where it projects to nothing.
                Geometry::Point(*p)
            }
            Geometry::MultiPoint(m) => Geometry::MultiPoint(m.iter().filter(|p| self.turn(p.x(), p.y()).2 >= -EPS).copied().collect()),
            Geometry::Line(l) => self.visible(&Geometry::LineString(LineString::new(vec![l.start, l.end]))),
            Geometry::LineString(l) => Geometry::MultiLineString(MultiLineString::new(self.lines(l))),
            Geometry::MultiLineString(m) => Geometry::MultiLineString(MultiLineString::new(m.iter().flat_map(|l| self.lines(l)).collect())),
            Geometry::Polygon(p) => Geometry::MultiPolygon(MultiPolygon::new(self.polygons(p))),
            Geometry::MultiPolygon(m) => Geometry::MultiPolygon(MultiPolygon::new(m.iter().flat_map(|p| self.polygons(p)).collect())),
            Geometry::Rect(r) => self.visible(&Geometry::Polygon(r.to_polygon())),
            Geometry::Triangle(t) => self.visible(&Geometry::Polygon(t.to_polygon())),
            Geometry::GeometryCollection(c) => Geometry::GeometryCollection(c.iter().map(|g| self.visible(g)).collect()),
        }
    }

    /// A line's points on the sphere, its long edges split.
    fn dense(&self, l: &LineString<f64>) -> Vec<V3> {
        let mut out = Vec::with_capacity(l.0.len());
        for (i, c) in l.0.iter().enumerate() {
            if i > 0 {
                let a = l.0[i - 1];
                // The same point twice (−180° and 180°, or a pole) is no edge.
                let (pa, pc) = (self.turn(a.x, a.y), self.turn(c.x, c.y));
                let same = (pa.0 - pc.0).abs() + (pa.1 - pc.1).abs() + (pa.2 - pc.2).abs() < 1e-12;
                let n = if same { 1 } else { ((c.x - a.x).abs().max((c.y - a.y).abs()) / STEP).ceil() as usize };
                for k in 1..n {
                    let t = k as f64 / n as f64;
                    out.push(self.turn(a.x + (c.x - a.x) * t, a.y + (c.y - a.y) * t));
                }
            }
            out.push(self.turn(c.x, c.y));
        }
        out
    }

    /// A line's runs in sight.
    fn lines(&self, l: &LineString<f64>) -> Vec<LineString<f64>> {
        let pts = self.dense(l);
        let mut runs = Vec::new();
        let mut run: Vec<Coord<f64>> = Vec::new();
        for (i, &p) in pts.iter().enumerate() {
            let seen = p.2 >= EPS;
            if i > 0 {
                let q = pts[i - 1];
                if (q.2 >= EPS) != seen {
                    run.push(self.coord(cross(q, p)));
                    if !seen {
                        runs.push(LineString::new(std::mem::take(&mut run)));
                    }
                }
            }
            if seen {
                run.push(self.coord(p));
            }
        }
        if run.len() >= 2 {
            runs.push(LineString::new(run));
        }
        runs.retain(|r| r.0.len() >= 2);
        runs
    }

    fn coord(&self, p: V3) -> Coord<f64> {
        let (x, y) = self.back(p);
        Coord { x, y }
    }

    /// A ring's parts in sight, each closed along the horizon where it was
    /// cut, as rings on the sphere. `ccw`: the ring turns counter-clockwise
    /// (an outside), else clockwise (a hole).
    ///
    /// Weiler–Atherton against the horizon: the ring's runs in sight, each
    /// from where it comes into sight to where it leaves; from where a run
    /// leaves, the horizon the ring's way round (the area in sight is the
    /// ring's inside on the disc, whose rim, as part of its edge, turns the
    /// ring's way) to where the next run comes back, and so on until the
    /// first is reached again. A ring that leaves and comes back at the same
    /// point (an area as wide as the world, down one side of the
    /// antimeridian and up the other) is closed by the whole rim.
    fn rings(&self, l: &LineString<f64>, ccw: bool) -> Vec<Vec<V3>> {
        let mut pts = self.dense(l);
        if pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        let n = pts.len();
        if n < 3 {
            return Vec::new();
        }
        let seen = |p: &V3| p.2 >= EPS;
        if pts.iter().all(seen) {
            return vec![pts];
        }
        if !pts.iter().any(seen) {
            // Out of sight all round: the whole disc when the ring holds the
            // view's centre, else nothing.
            return if self.holds_centre(l) { vec![rim(ccw)] } else { Vec::new() };
        }
        // The runs in sight: (where it comes back, the points, where it leaves).
        let start = (0..n).find(|&i| !seen(&pts[i])).unwrap();
        let mut runs: Vec<(V3, Vec<V3>, V3)> = Vec::new();
        let mut current: Option<(V3, Vec<V3>)> = None;
        for k in 1..=n {
            let (q, p) = (pts[(start + k - 1) % n], pts[(start + k) % n]);
            match (seen(&q), seen(&p)) {
                (false, true) => current = Some((cross(q, p), vec![inward(p)])),
                (true, true) => {
                    if let Some((_, run)) = current.as_mut() {
                        run.push(inward(p));
                    }
                }
                (true, false) => {
                    if let Some((entry, run)) = current.take() {
                        runs.push((entry, run, cross(q, p)));
                    }
                }
                (false, false) => {}
            }
        }
        let angle = |p: &V3| p.1.atan2(p.0);
        // How far round the rim, the ring's way, from `a` to `b`: (0, 2π].
        let along = |a: &V3, b: &V3| {
            let d = if ccw { angle(b) - angle(a) } else { angle(a) - angle(b) };
            let d = d.rem_euclid(std::f64::consts::TAU);
            if d < 1e-9 { std::f64::consts::TAU } else { d }
        };
        let mut used = vec![false; runs.len()];
        let mut out = Vec::new();
        for first in 0..runs.len() {
            if used[first] {
                continue;
            }
            let mut ring: Vec<V3> = Vec::new();
            let mut i = first;
            for _ in 0..=runs.len() {
                used[i] = true;
                let (entry, run, exit) = &runs[i];
                ring.push(*entry);
                ring.extend(run.iter().copied());
                ring.push(*exit);
                // The run whose way back is met first round the rim.
                let next = (0..runs.len()).min_by(|&a, &b| along(exit, &runs[a].0).total_cmp(&along(exit, &runs[b].0))).unwrap();
                ring.extend(horizon(*exit, along(exit, &runs[next].0) * if ccw { 1.0 } else { -1.0 }));
                if next == first {
                    break;
                }
                if used[next] {
                    break;
                }
                i = next;
            }
            if ring.len() >= 3 {
                out.push(ring);
            }
        }
        out
    }

    /// Whether the ring, in lon/lat, holds the view's centre.
    fn holds_centre(&self, l: &LineString<f64>) -> bool {
        let lat0 = self.sin0.atan2(self.cos0).to_degrees();
        let (x, y) = (self.lon0, lat0);
        let mut inside = false;
        for w in l.0.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.y > y) != (b.y > y) && x < a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x) {
                inside = !inside;
            }
        }
        inside
    }

    fn to_ring(&self, pts: Vec<V3>) -> LineString<f64> {
        let mut ring: Vec<Coord<f64>> = pts.into_iter().map(|p| self.coord(p)).collect();
        ring.push(ring[0]);
        LineString::new(ring)
    }

    /// A polygon's parts in sight: each outside cut, with the holes cut and
    /// given to the part they lie in.
    fn polygons(&self, p: &Polygon<f64>) -> Vec<Polygon<f64>> {
        // Turned by role, whatever way the data drew them: the outside
        // counter-clockwise, the holes clockwise.
        let turned = |l: &LineString<f64>, ccw: bool| {
            if (signed_area(l) > 0.0) == ccw {
                l.clone()
            } else {
                LineString::new(l.0.iter().rev().copied().collect())
            }
        };
        let outsides = self.rings(&turned(p.exterior(), true), true);
        if outsides.is_empty() {
            return Vec::new();
        }
        let holes: Vec<Vec<V3>> = p.interiors().iter().flat_map(|h| self.rings(&turned(h, false), false)).collect();
        // On the disc (x, y) a hole is in the outside that holds a point of it.
        let flat = |r: &[V3]| LineString::new(r.iter().map(|p| Coord { x: p.0, y: p.1 }).collect::<Vec<_>>());
        let shapes: Vec<Polygon<f64>> = outsides.iter().map(|o| Polygon::new(flat(o), vec![])).collect();
        let mut given: Vec<Vec<Vec<V3>>> = vec![Vec::new(); outsides.len()];
        for h in holes {
            let mid = h.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
            let at = geo::Point::new(mid.0 / h.len() as f64, mid.1 / h.len() as f64);
            let k = shapes.iter().position(|s| geo::Contains::contains(s, &at)).unwrap_or(0);
            given[k].push(h);
        }
        outsides
            .into_iter()
            .zip(given)
            .map(|(o, hs)| Polygon::new(self.to_ring(o), hs.into_iter().map(|h| self.to_ring(h)).collect()))
            .collect()
    }
}

/// How far out from the centre a cut ring's points may lie, on the disc: a
/// chord of the rim a degree long comes this far in at its middle. A shore
/// nearer the rim than that would cross the rim's chords, and a ring that
/// crosses itself is no area to a union.
const RIM_IN: f64 = 0.999_961_923_064_171; // cos 0.5°

/// A point in sight, moved in to [`RIM_IN`] when it lies nearer the rim
/// (at most 240 m on the ground).
fn inward(p: V3) -> V3 {
    let r = p.0.hypot(p.1);
    if r <= RIM_IN * (1.0 - EPS * EPS).sqrt() {
        return p;
    }
    let k = RIM_IN * (1.0 - EPS * EPS).sqrt() / r;
    let (x, y) = (p.0 * k, p.1 * k);
    (x, y, (1.0 - x * x - y * y).max(0.0).sqrt())
}

/// The whole rim, a point a degree, counter-clockwise or not.
fn rim(ccw: bool) -> Vec<V3> {
    let r = (1.0 - EPS * EPS).sqrt();
    (0..360)
        .map(|k| {
            let t = (k as f64).to_radians() * if ccw { 1.0 } else { -1.0 };
            (r * t.cos(), r * t.sin(), EPS)
        })
        .collect()
}

/// `lon` in −180..180.
fn wrap(lon: f64) -> f64 {
    let l = (lon + 180.0).rem_euclid(360.0) - 180.0;
    if l == -180.0 && lon > 0.0 { 180.0 } else { l }
}

/// Where the edge from `a` to `b` meets the horizon (a hair inside it).
fn cross(a: V3, b: V3) -> V3 {
    let t = ((EPS - a.2) / (b.2 - a.2)).clamp(0.0, 1.0);
    let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, EPS);
    let r = (p.0 * p.0 + p.1 * p.1).sqrt().max(1e-12);
    let k = (1.0 - EPS * EPS).sqrt() / r;
    (p.0 * k, p.1 * k, EPS)
}

/// Twice a ring's signed area in lon/lat: positive counter-clockwise.
fn signed_area(l: &LineString<f64>) -> f64 {
    l.0.windows(2).map(|w| w[0].x * w[1].y - w[1].x * w[0].y).sum()
}

/// The horizon from `a`, `sweep` radians round (counter-clockwise when
/// positive), to just before the end: a point a degree.
fn horizon(a: V3, sweep: f64) -> Vec<V3> {
    let ta = a.1.atan2(a.0);
    let n = (sweep.abs().to_degrees() / STEP).ceil() as usize;
    let r = (1.0 - EPS * EPS).sqrt();
    (1..n)
        .map(|k| {
            let t = ta + sweep * k as f64 / n as f64;
            (r * t.cos(), r * t.sin(), EPS)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Area;

    #[test]
    fn the_centre_is_the_origin_and_the_far_side_is_hidden() {
        let o = Ortho::new(12.0, 42.0);
        let (x, y) = o.forward(12.0, 42.0);
        assert!(x.abs() < 1e-6 && y.abs() < 1e-6);
        let (x, y) = o.forward(12.0 + 180.0, -42.0);
        assert!(x.is_nan() && y.is_nan(), "the antipode");
        let (x, y) = o.forward(13.0, 42.0);
        assert!(x > 80_000.0 && y.abs() < 2_000.0, "east is right: {x} {y}");
        let (lon, lat) = o.inverse(o.forward(40.0, 10.0).0, o.forward(40.0, 10.0).1).unwrap();
        assert!((lon - 40.0).abs() < 1e-9 && (lat - 10.0).abs() < 1e-9);
        assert_eq!(o.inverse(RADIUS * 1.01, 0.0), None);
    }

    #[test]
    fn parsed_from_a_proj_string() {
        let o = Ortho::parse("+proj=ortho +lat_0=42.5 +lon_0=-10").unwrap();
        assert_eq!(o, Ortho::new(-10.0, 42.5));
        assert_eq!(Ortho::parse("+proj=laea +lat_0=1"), None);
    }

    #[test]
    fn a_line_over_the_horizon_is_cut_there() {
        // Along the equator from the centre round to the far side.
        let o = Ortho::new(0.0, 0.0);
        let l = LineString::from(vec![(0.0, 0.0), (170.0, 0.0)]);
        let Geometry::MultiLineString(m) = o.visible(&Geometry::LineString(l)) else { panic!() };
        assert_eq!(m.0.len(), 1);
        let end = m.0[0].0.last().unwrap();
        assert!((end.x - 90.0).abs() < 0.01, "{end:?}");
    }

    #[test]
    fn an_area_over_the_horizon_is_closed_along_it() {
        // A band from 60° west to 150° east, seen from 0°: cut at 90° east,
        // its edge there the horizon.
        let o = Ortho::new(0.0, 0.0);
        let band = Polygon::new(LineString::from(vec![(-60.0, -30.0), (150.0, -30.0), (150.0, 30.0), (-60.0, 30.0), (-60.0, -30.0)]), vec![]);
        let Geometry::MultiPolygon(m) = o.visible(&Geometry::Polygon(band)) else { panic!() };
        assert_eq!(m.0.len(), 1);
        let xs: Vec<f64> = m.0[0].exterior().0.iter().map(|c| c.x).collect();
        let east = xs.iter().cloned().fold(f64::MIN, f64::max);
        assert!((east - 90.0).abs() < 0.01, "{east}");
        // Projected, it is the band from −60° to the rim: a real area.
        let projected = m.0[0].exterior().0.iter().map(|c| o.forward(c.x, c.y)).collect::<Vec<_>>();
        assert!(projected.iter().all(|p| p.0.is_finite()));
        let area = Polygon::new(LineString::from(projected), vec![]).unsigned_area();
        assert!(area > 1e13, "{area}");
        // Wholly hidden, nothing.
        let far = Polygon::new(LineString::from(vec![(120.0, -10.0), (150.0, -10.0), (150.0, 10.0), (120.0, 10.0), (120.0, -10.0)]), vec![]);
        let Geometry::MultiPolygon(m) = o.visible(&Geometry::Polygon(far)) else { panic!() };
        assert!(m.0.is_empty());
    }

    #[test]
    fn a_ring_either_way_round_closes_on_the_same_side() {
        // A sea from 60° west to 150° east and 80° south to 10° north, seen
        // from 0°, 0°: cut at the rim on the east. Drawn clockwise or
        // counter-clockwise, its part in sight is the same.
        let o = Ortho::new(0.0, 0.0);
        let ring = vec![(-60.0, -80.0), (150.0, -80.0), (150.0, 10.0), (-60.0, 10.0), (-60.0, -80.0)];
        let area = |pts: Vec<(f64, f64)>| {
            let Geometry::MultiPolygon(m) = o.visible(&Geometry::Polygon(Polygon::new(LineString::from(pts), vec![]))) else { panic!() };
            m.0.iter().map(|p| Polygon::new(LineString::from(p.exterior().0.iter().map(|c| o.forward(c.x, c.y)).collect::<Vec<_>>()), vec![]).unsigned_area()).sum::<f64>()
        };
        let (ccw, cw) = (area(ring.clone()), area(ring.into_iter().rev().collect()));
        assert!(ccw > 1e13 && (ccw - cw).abs() < ccw * 1e-6, "{ccw} {cw}");
        // Less than the half disc south of the equator plus a strip: not the
        // whole disc, as an arc the long way round would make it.
        assert!(ccw < 0.75 * std::f64::consts::PI * RADIUS * RADIUS, "{ccw}");
    }

    #[test]
    fn an_area_as_wide_as_the_world_fills_the_disc() {
        // A world tile's sea, −180° to 180°: it leaves sight down the
        // antimeridian and comes back up it. Either way round, the disc.
        let o = Ortho::new(-40.0, 30.0);
        let disc = std::f64::consts::PI * RADIUS * RADIUS;
        let world = vec![(-180.0, -85.0), (180.0, -85.0), (180.0, 85.0), (-180.0, 85.0), (-180.0, -85.0)];
        for ring in [world.clone(), world.into_iter().rev().collect()] {
            let Geometry::MultiPolygon(m) = o.visible(&Geometry::Polygon(Polygon::new(LineString::from(ring), vec![]))) else { panic!() };
            let a: f64 = m.0.iter().map(|p| Polygon::new(LineString::from(p.exterior().0.iter().map(|c| o.forward(c.x, c.y)).collect::<Vec<_>>()), vec![]).unsigned_area()).sum();
            assert!((a / disc - 1.0).abs() < 1e-3, "{}", a / disc);
        }
    }

    #[test]
    fn a_far_area_reaching_into_sight_twice_is_two_pieces() {
        // Seen from 0°, 0°, the horizon is the 90th meridian. A sea beyond
        // it, with two fingers reaching west to 80°: in sight, the fingers
        // alone, not the rim joined round the far side of the disc.
        let o = Ortho::new(0.0, 0.0);
        let sea = vec![
            (95.0, -40.0), (140.0, -40.0), (140.0, 40.0), (95.0, 40.0), (95.0, 30.0), (80.0, 30.0), (80.0, 20.0), (95.0, 20.0),
            (95.0, -20.0), (80.0, -20.0), (80.0, -30.0), (95.0, -30.0), (95.0, -40.0),
        ];
        let Geometry::MultiPolygon(m) = o.visible(&Geometry::Polygon(Polygon::new(LineString::from(sea), vec![]))) else { panic!() };
        assert_eq!(m.0.len(), 2);
        let disc = std::f64::consts::PI * RADIUS * RADIUS;
        let a: f64 = m.0.iter().map(|p| Polygon::new(LineString::from(p.exterior().0.iter().map(|c| o.forward(c.x, c.y)).collect::<Vec<_>>()), vec![]).unsigned_area()).sum();
        assert!(a > 0.0 && a < 0.02 * disc, "{}", a / disc);
    }

    #[test]
    fn a_hidden_ring_round_the_centre_is_the_disc() {
        // All of it beyond the horizon, but round the view's centre: a sea
        // whose shore is a far ring holds the whole near side.
        let o = Ortho::new(0.0, 89.0);
        let ring = LineString::from(vec![(-180.0, -10.0), (180.0, -10.0), (180.0, 89.5), (-180.0, 89.5), (-180.0, -10.0)]);
        assert!(o.holds_centre(&ring));
    }

    #[test]
    fn a_shore_along_the_rim_still_makes_an_area() {
        // Seen from 0°, 0°, the horizon is the 90th meridian. A sea from
        // beyond it whose shore comes into sight and runs a hair inside the
        // rim (89.995°, a metre of disc) for five degrees: the rim's chords,
        // a degree each, sag further in than that, so the cut must keep its
        // shore off them, or it crosses itself and a union drops it.
        let o = Ortho::new(0.0, 0.0);
        let mut ring = vec![(95.0, -3.0)];
        ring.extend((0..=10).map(|k| (89.995, -2.5 + k as f64 * 0.5)));
        ring.extend([(95.0, 3.0), (95.0, -3.0)]);
        let Geometry::MultiPolygon(m) = o.visible(&Geometry::Polygon(Polygon::new(LineString::from(ring), vec![]))) else { panic!() };
        let pr = |l: &LineString<f64>| LineString::from(l.0.iter().map(|c| { let (x, y) = o.forward(c.x, c.y); (x, -y) }).collect::<Vec<_>>());
        let cut = geo::MultiPolygon::new(m.0.iter().map(|p| Polygon::new(pr(p.exterior()), vec![])).collect());
        use geo::Validation;
        assert!(cut.is_valid(), "the cut crosses itself");
        let united = crate::geom::union_multi([&cut]);
        assert!(cut.unsigned_area() > 0.0);
        assert!((united.unsigned_area() / cut.unsigned_area() - 1.0).abs() < 1e-3, "{} {}", united.unsigned_area(), cut.unsigned_area());
    }

    #[test]
    fn a_ring_that_starts_out_of_sight_still_closes() {
        let o = Ortho::new(0.0, 0.0);
        let band = Polygon::new(LineString::from(vec![(150.0, -30.0), (150.0, 30.0), (-60.0, 30.0), (-60.0, -30.0), (150.0, -30.0)]), vec![]);
        let Geometry::MultiPolygon(m) = o.visible(&Geometry::Polygon(band)) else { panic!() };
        assert_eq!(m.0.len(), 1);
        assert!(m.0[0].exterior().0.iter().all(|c| c.x.is_finite() && c.y.is_finite()));
        let projected = m.0[0].exterior().0.iter().map(|c| o.forward(c.x, c.y)).collect::<Vec<_>>();
        assert!(Polygon::new(LineString::from(projected), vec![]).unsigned_area() > 1e13);
    }
}
