//! A screen that moves: the graph painted once for an area wider than the
//! screen (a [`Scene`]), then any [`View`] of it, moved or zoomed, painted
//! from that geometry alone, in the time a frame has. Only the rasterising
//! and the names' places are done again; the graph is not run. A view that
//! leaves the scene, or zooms far from its scale, asks for a new one
//! ([`Scene::covers`]); until it comes, the old one is painted, stretched.
//!
//! Scenes are web Mercator, north up: a page there is the Mercator plane
//! scaled at its centre, so a view of it is the same geometry moved and
//! scaled.

use crate::Picture;
use drawland_core::grammar_run::{PaintShape, Painted};
use drawland_core::labels::{Named, Shape};
use drawland_core::output::Output;
use drawland_core::paint::Screen;
use drawland_core::profile::ProjectionMode;
use geo::{BoundingRect, Coord, LineString, MapCoords, MultiPolygon, Polygon, Rect};

const R: f64 = 6_378_137.0;

/// `(lon, lat)` in degrees to web Mercator metres.
pub fn mercator(lon: f64, lat: f64) -> (f64, f64) {
    let lat = lat.clamp(-85.05, 85.05);
    (R * lon.to_radians(), R * (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.0).tan().ln())
}

/// Web Mercator metres to `(lon, lat)` in degrees.
pub fn geographic((x, y): (f64, f64)) -> (f64, f64) {
    ((x / R).to_degrees(), (2.0 * (y / R).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees())
}

/// What a screen shows: where its centre is, how near, how big.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The centre, web Mercator metres.
    pub center: (f64, f64),
    /// Mercator metres a unit (a pixel, or a character across).
    pub per_unit: f64,
    /// The screen: its kind and size.
    pub screen: Screen,
}

impl View {
    /// The view at `(lon, lat)` where a unit spans `scale` mm of ground
    /// (the output's scale, true at the centre's latitude).
    pub fn at(lon: f64, lat: f64, scale: f64, screen: Screen) -> View {
        let lat = lat.clamp(-85.05, 85.05);
        View { center: mercator(lon, lat), per_unit: scale / 1000.0 / lat.to_radians().cos(), screen }
    }

    /// An output's view, when it is web Mercator, north up and unbent.
    pub fn of(output: &Output, screen: Screen) -> Option<View> {
        let plain = output.projection == ProjectionMode::Webmercator && output.bearing.rem_euclid(360.0) == 0.0;
        plain.then(|| View::at(output.center.0, output.center.1, output.scale, screen))
    }

    /// The screen's top-left dot in the world's dots: where patterns and
    /// tiled characters are read, so they stay on the ground as it moves.
    pub fn origin(&self) -> (i64, i64) {
        let per_dot = self.per_unit * self.screen.dot();
        let (w, h) = self.screen.dots();
        ((self.center.0 / per_dot).round() as i64 - w as i64 / 2, (-self.center.1 / per_dot).round() as i64 - h as i64 / 2)
    }

    /// The vector tiles' zoom whose detail is the view's: a tile's pixel
    /// (a 512-pixel tile) no wider than a dot. A web map picks its tiles so,
    /// which keeps a scene a few dozen tiles at any scale; deeper than the
    /// tile source goes, the source's deepest is read.
    pub fn tile_zoom(&self) -> u8 {
        let world = 2.0 * std::f64::consts::PI * R;
        (world / (512.0 * self.per_unit * self.screen.dot())).log2().ceil().clamp(0.0, 22.0) as u8
    }
}

/// The layers of a wide screen, each piece with its box, to be painted as
/// any view of it.
#[derive(Clone, Debug)]
pub struct Scene {
    view: View,
    layers: Vec<Painted>,
    /// Each layer's pieces' boxes (polygons, lines, names), in its units.
    boxes: Vec<Vec<Rect<f64>>>,
}

impl Scene {
    /// The layers painted for `view` (its screen the scene's whole area).
    pub fn new(view: View, layers: Vec<Painted>) -> Scene {
        let none = || Rect::new(Coord { x: f64::NAN, y: f64::NAN }, Coord { x: f64::NAN, y: f64::NAN });
        let boxes = layers
            .iter()
            .map(|l| match &l.shape {
                PaintShape::Area(a) => a.0.iter().map(|p| p.bounding_rect().unwrap_or_else(none)).collect(),
                PaintShape::Lines(ls) => ls.iter().map(|l| l.bounding_rect().unwrap_or_else(none)).collect(),
                PaintShape::Names(ns) => ns.iter().map(|n| shape_box(&n.shape).unwrap_or_else(none)).collect(),
            })
            .collect();
        Scene { view, layers, boxes }
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    /// Whether `view` is well inside this scene: its scale within half a
    /// zoom step, and a quarter of a screen round it still held.
    pub fn covers(&self, view: &View) -> bool {
        let k = self.view.per_unit / view.per_unit;
        if !(std::f64::consts::FRAC_1_SQRT_2..=std::f64::consts::SQRT_2).contains(&k) {
            return false;
        }
        let (sw, sh) = self.view.screen.size();
        let (vw, vh) = view.screen.size();
        // The view's half size and its margin, in Mercator metres.
        let (hx, hy) = (0.75 * vw * view.per_unit, 0.75 * vh * view.per_unit);
        let (dx, dy) = ((view.center.0 - self.view.center.0).abs(), (view.center.1 - self.view.center.1).abs());
        dx + hx <= sw / 2.0 * self.view.per_unit && dy + hy <= sh / 2.0 * self.view.per_unit
    }

    /// `view` painted from this scene's geometry, and how far the picture
    /// is off the view, in units: the map is moved by whole cells (pixels on
    /// a bitmap, characters on text), so names, lines and patterns keep
    /// their places on it from one frame to the next, and a host shows the
    /// picture moved by this much to put it where the view is.
    pub fn render(&self, view: &View) -> (Picture, (f64, f64)) {
        let k = self.view.per_unit / view.per_unit;
        let (sw, sh) = self.view.screen.size();
        let (vw, vh) = view.screen.size();
        let exact = Coord {
            x: vw / 2.0 - sw / 2.0 * k + (self.view.center.0 - view.center.0) / view.per_unit,
            y: vh / 2.0 - sh / 2.0 * k - (self.view.center.1 - view.center.1) / view.per_unit,
        };
        let (cw, ch) = view.screen.cell();
        let t = Coord { x: (exact.x / cw).round() * cw, y: (exact.y / ch).round() * ch };
        // Patterns read where the moved map puts them: the origin follows
        // the same whole steps as the geometry.
        let dot = view.screen.dot();
        let per_dot = view.per_unit * dot;
        let (wd, hd) = view.screen.dots();
        let origin = (
            ((exact.x / dot + view.center.0 / per_dot).round() - t.x / dot) as i64 - wd as i64 / 2,
            ((exact.y / dot - view.center.1 / per_dot).round() - t.y / dot) as i64 - hd as i64 / 2,
        );
        let to = |c: Coord<f64>| Coord { x: c.x * k + t.x, y: c.y * k + t.y };
        // The view in the scene's units, a margin round it for halos and
        // names that start outside it.
        let margin = 32.0 / k;
        let (x0, y0) = ((-t.x) / k - margin, (-t.y) / k - margin);
        let (x1, y1) = ((vw - t.x) / k + margin, (vh - t.y) / k + margin);
        let seen = |r: &Rect<f64>| r.max().x >= x0 && r.min().x <= x1 && r.max().y >= y0 && r.min().y <= y1;
        let layers: Vec<Painted> = self
            .layers
            .iter()
            .zip(&self.boxes)
            .map(|(l, boxes)| {
                let shape = match &l.shape {
                    PaintShape::Area(a) => PaintShape::Area(MultiPolygon::new(
                        a.0.iter().zip(boxes).filter(|(_, b)| seen(b)).map(|(p, _)| p.map_coords(to)).collect::<Vec<Polygon<f64>>>(),
                    )),
                    PaintShape::Lines(ls) => {
                        PaintShape::Lines(ls.iter().zip(boxes).filter(|(_, b)| seen(b)).map(|(l, _)| l.map_coords(to)).collect::<Vec<LineString<f64>>>())
                    }
                    PaintShape::Names(ns) => PaintShape::Names(
                        ns.iter()
                            .zip(boxes)
                            .filter(|(_, b)| seen(b))
                            .map(|(n, _)| Named { shape: moved(&n.shape, &to), extent: n.extent * k, ..n.clone() })
                            .collect(),
                    ),
                };
                Painted { id: l.id.clone(), look: l.look.clone(), halo: l.halo, shape }
            })
            .collect();
        (crate::render(&view.screen, origin, &layers), (exact.x - t.x, exact.y - t.y))
    }
}

fn shape_box(shape: &Shape) -> Option<Rect<f64>> {
    match shape {
        Shape::Road(points) => LineString::new(points.clone()).bounding_rect(),
        Shape::Area(p) => p.bounding_rect(),
        Shape::Pin(c) => Some(Rect::new(*c, *c)),
    }
}

fn moved(shape: &Shape, to: &impl Fn(Coord<f64>) -> Coord<f64>) -> Shape {
    match shape {
        Shape::Road(points) => Shape::Road(points.iter().map(|&c| to(c)).collect()),
        Shape::Area(p) => Shape::Area(p.map_coords(to)),
        Shape::Pin(c) => Shape::Pin(to(*c)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drawland_core::paint::{FillPaint, Halo, Look, ScreenKind};

    fn square(x: f64, y: f64, s: f64) -> Polygon<f64> {
        Polygon::new(LineString::from(vec![(x, y), (x + s, y), (x + s, y + s), (x, y + s), (x, y)]), vec![])
    }

    #[test]
    fn a_view_is_the_scene_moved_and_scaled() {
        let wide = Screen { kind: ScreenKind::Bitmap, width: 40, height: 40, invert: false };
        let small = Screen { width: 10, height: 10, ..wide };
        let at = View::at(12.0, 45.0, 3000.0, wide);
        // A black square 4 pixels wide at the scene's middle.
        let solid = Look::Fill(FillPaint { pattern: Some("solid".into()), ..FillPaint::default() });
        let layers = vec![Painted { id: "a".into(), look: solid, halo: Halo(0, 0), shape: PaintShape::Area(MultiPolygon::new(vec![square(18.0, 18.0, 4.0)])) }];
        let scene = Scene::new(at, layers);
        let black = |v: &View| match scene.render(v).0 {
            Picture::Bitmap(b) => (0..10).flat_map(|y| (0..10).map(move |x| (x, y))).filter(|&(x, y)| b.black(x, y)).collect::<Vec<_>>(),
            _ => unreachable!(),
        };
        // Same centre, a smaller screen: the square in its middle.
        let same = View { screen: small, ..at };
        assert_eq!(black(&same), (3..7).flat_map(|y| (3..7).map(move |x| (x, y))).collect::<Vec<_>>());
        // Moved two pixels east: the square two pixels west.
        let east = View { center: (at.center.0 + 2.0 * at.per_unit, at.center.1), ..same };
        assert_eq!(black(&east).first(), Some(&(1, 3)));
        // Twice as near: twice as big, still centred.
        let near = View { per_unit: at.per_unit / 2.0, ..same };
        assert_eq!(black(&near).len(), 64);
        // Half a pixel east: the map moved one whole pixel, the picture
        // said to be half a pixel off.
        let half = View { center: (at.center.0 + 0.5 * at.per_unit, at.center.1), ..same };
        let (_, off) = scene.render(&half);
        assert!((off.0.abs() - 0.5).abs() < 1e-9 && off.1.abs() < 1e-9);
        assert_eq!(at.tile_zoom(), 15);
        assert!(View { per_unit: at.per_unit * 64.0, ..at }.tile_zoom() == 9);
        assert!(scene.covers(&same) && !scene.covers(&near) && !scene.covers(&View { center: (at.center.0 + 40.0 * at.per_unit, at.center.1), ..same }));
    }
}
