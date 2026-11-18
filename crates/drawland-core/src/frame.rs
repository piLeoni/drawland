//! The frame a profile draws in: the page (paper, in mm) and the world (the
//! ground under it), and the way between them. Everything here follows from
//! the output alone — the place, the paper, the scale, the bearing, the
//! projection and the warp — never from downloaded data, so a script can ask
//! it while it runs (`page`, `world`), before anything is fetched.
//!
//! `to_page` is where the drawn map puts a place: the output's projection,
//! then its warp. `to_world` undoes both.

use crate::output::Output;
use crate::profile::OutputSpec;
use crate::projection::Projector;
use crate::warp::Warps;
use geo::Coord;

/// An output's page and ground, and the way between them.
#[derive(Debug, Clone)]
pub struct Frame {
    pub output: Output,
    projector: Projector,
    warp: Warps,
}

/// The ground's extremes under the page, in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub north: f64,
    pub south: f64,
    pub east: f64,
    pub west: f64,
}

impl Frame {
    /// The frame of `spec` centred on `(lon, lat)`.
    pub fn new(spec: &OutputSpec, center: (f64, f64)) -> Frame {
        let output = Output::new(spec, center);
        let projector = Projector::new(&output);
        Frame { output, projector, warp: spec.warp.clone() }
    }

    /// The warps after the projection.
    pub fn warp(&self) -> &Warps {
        &self.warp
    }

    /// Where `(lon, lat)` is drawn, in page mm from the paper's top left.
    pub fn to_page(&self, lon: f64, lat: f64) -> (f64, f64) {
        let (x, y) = self.projector.project(lon, lat);
        let c = self.warp.point_at(self.output.width_mm, self.output.height_mm, Coord { x, y });
        (c.x, c.y)
    }

    /// The `(lon, lat)` drawn at page mm `(x, y)`: the warp undone in page
    /// space, then the projection. `None` where either does not settle (off
    /// the edge of the projection's world, or a place the warp sends nothing).
    pub fn to_world(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let (ux, uy) = self.unwarp(x, y)?;
        self.projector.locate(ux, uy)
    }

    /// The `(lon, lat)` drawn at page mm `(x, y)` when the drawing shows
    /// ground there: as `to_world`, but `None` outside the bent frame, where a
    /// warp that moves the page's edge (a mesh) is only carried on.
    pub fn to_world_drawn(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let (ux, uy) = self.unwarp(x, y)?;
        const EPS: f64 = 1e-6;
        let (w, h) = (self.output.width_mm, self.output.height_mm);
        if self.warp.moves_edge() && !((-EPS..=w + EPS).contains(&ux) && (-EPS..=h + EPS).contains(&uy)) {
            return None;
        }
        self.projector.locate(ux, uy)
    }

    /// The point the warp carries to `(x, y)`: Newton steps on the warp from
    /// `(x, y)` itself, since a warp moves most points only a little.
    fn unwarp(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        if self.warp.is_empty() {
            return Some((x, y));
        }
        let (w, h) = (self.output.width_mm, self.output.height_mm);
        let at = |x: f64, y: f64| {
            let c = self.warp.point_at(w, h, Coord { x, y });
            (c.x, c.y)
        };
        let (mut ux, mut uy) = (x, y);
        const H: f64 = 1e-6;
        for _ in 0..60 {
            let (px, py) = at(ux, uy);
            let (ex, ey) = (px - x, py - y);
            if ex.hypot(ey) < 1e-9 {
                return Some((ux, uy));
            }
            let (ax, ay) = at(ux + H, uy);
            let (bx, by) = at(ux, uy + H);
            let (j11, j21, j12, j22) = ((ax - px) / H, (ay - py) / H, (bx - px) / H, (by - py) / H);
            let det = j11 * j22 - j12 * j21;
            if !det.is_finite() || det.abs() < 1e-12 {
                return None;
            }
            ux -= (j22 * ex - j12 * ey) / det;
            uy -= (-j21 * ex + j11 * ey) / det;
            if !(ux.is_finite() && uy.is_finite()) {
                return None;
            }
        }
        let (px, py) = at(ux, uy);
        ((px - x).hypot(py - y) < 1e-5).then_some((ux, uy))
    }

    /// `90` or `-90` when that pole is drawn on the page.
    pub fn pole(&self) -> Option<f64> {
        let (w, h) = (self.output.width_mm, self.output.height_mm);
        [90.0, -90.0].into_iter().find(|&lat| {
            let (x, y) = self.to_page(0.0, lat);
            x.is_finite() && y.is_finite() && (0.0..=w).contains(&x) && (0.0..=h).contains(&y)
        })
    }

    /// The ground's extremes under the whole page, found round its edge (at
    /// any bearing, in any projection); with a pole on the page, up to it and
    /// every longitude. The antimeridian alone on the page is not looked for.
    pub fn bounds(&self) -> Bounds {
        const PER_SIDE: usize = 32;
        let (w, h) = (self.output.width_mm, self.output.height_mm);
        let mut b = Bounds { north: f64::MIN, south: f64::MAX, east: f64::MIN, west: f64::MAX };
        for i in 0..PER_SIDE * 4 {
            let t = (i % PER_SIDE) as f64 / PER_SIDE as f64;
            let (x, y) = match i / PER_SIDE {
                0 => (w * t, 0.0),
                1 => (w, h * t),
                2 => (w * (1.0 - t), h),
                _ => (0.0, h * (1.0 - t)),
            };
            if let Some((lon, lat)) = self.to_world(x, y) {
                b.north = b.north.max(lat);
                b.south = b.south.min(lat);
                b.east = b.east.max(lon);
                b.west = b.west.min(lon);
            }
        }
        if b.north < b.south {
            let (lon, lat) = self.output.center;
            b = Bounds { north: lat, south: lat, east: lon, west: lon };
        }
        // A pole on the page: the edge circles it, so it holds every longitude.
        if let Some(pole) = self.pole() {
            (b.west, b.east) = (-180.0, 180.0);
            if pole > 0.0 { b.north = 90.0 } else { b.south = -90.0 }
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(json: &str) -> OutputSpec {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_centre_is_the_middle_of_the_page() {
        let f = Frame::new(&spec(r#"{ "width": "200mm", "height": "100mm", "scale": 10000 }"#), (-71.06, 42.36));
        let (x, y) = f.to_page(-71.06, 42.36);
        assert!((x - 100.0).abs() < 1e-6 && (y - 50.0).abs() < 1e-6, "{x} {y}");
    }

    #[test]
    fn to_world_undoes_to_page_under_a_bearing_and_a_warp() {
        let f = Frame::new(
            &spec(r#"{ "width": "200mm", "height": "100mm", "scale": 25000, "bearing": 30, "warp": { "fisheye": 0.5 } }"#),
            (12.49, 41.89),
        );
        for (x, y) in [(10.0, 10.0), (150.0, 80.0), (100.0, 50.0), (199.0, 1.0), (200.0, 0.0), (0.0, 100.0)] {
            let (lon, lat) = f.to_world(x, y).expect("on the page");
            let (bx, by) = f.to_page(lon, lat);
            assert!((bx - x).hypot(by - y) < 1e-4, "({x}, {y}) came back as ({bx}, {by})");
        }
    }

    #[test]
    fn a_polar_page_reaches_the_pole() {
        let f = Frame::new(
            &spec(r#"{ "width": "297mm", "height": "210mm", "scale": 30000000, "projection": "+proj=stere +lat_0=90 +lon_0={lng} +lat_ts=90" }"#),
            (20.0, 80.0),
        );
        assert_eq!(f.pole(), Some(90.0));
        let b = f.bounds();
        assert_eq!((b.north, b.west, b.east), (90.0, -180.0, 180.0));
        assert!(b.south < 70.0, "{b:?}");
        // The meridian through the centre runs straight up the page to the pole.
        let (x, y) = f.to_page(20.0, 90.0);
        assert!((x - 148.5).abs() < 1e-6 && y < 105.0, "{x} {y}");
        let q = f.output.query_polygon();
        use geo::BoundingRect;
        let r = q.bounding_rect().unwrap();
        assert_eq!((r.min().x, r.max().x, r.max().y), (-180.0, 180.0, 90.0));
    }

    #[test]
    fn north_is_up_and_bounds_hold_the_centre() {
        let f = Frame::new(&spec(r#"{ "width": "297mm", "height": "210mm", "scale": 750000 }"#), (-70.54, 41.95));
        let b = f.bounds();
        assert!(b.south < 41.95 && 41.95 < b.north && b.west < -70.54 && -70.54 < b.east, "{b:?}");
        // 210 mm at 1:750 000 is 157.5 km north to south: about 1.42° of latitude.
        assert!(((b.north - b.south) - 157.5 / 111.1).abs() < 0.05, "{b:?}");
        let (_, y_north) = f.to_page(-70.54, b.north);
        assert!(y_north.abs() < 1e-3, "the north edge is the top of the page: {y_north}");
    }
}
