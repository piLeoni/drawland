//! The output: where it is, how big, at what scale — and the ground area and
//! Overpass query that follow from it.

use crate::profile::{OutputSpec, ProjectionMode};
use geo::{Coord, LineString, Polygon};
use geographiclib_rs::{DirectGeodesic, Geodesic};

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// Centre `(lon, lat)` in degrees.
    pub center: (f64, f64),
    pub width_mm: f64,
    pub height_mm: f64,
    /// Scale denominator (`1:10000` → `10000`).
    pub scale: f64,
    pub bearing: f64,
    pub projection: ProjectionMode,
    /// The vector tiles' zoom when the document sets it (`tileZoom`).
    pub tile_zoom: Option<u8>,
}

impl Output {
    pub fn new(map: &OutputSpec, center: (f64, f64)) -> Output {
        Output {
            center,
            width_mm: map.width.0,
            height_mm: map.height.0,
            scale: map.scale.0,
            bearing: map.bearing,
            projection: map.projection.clone(),
            tile_zoom: map.tile_zoom,
        }
    }

    /// Output size on the ground, metres.
    pub fn ground_size_m(&self) -> (f64, f64) {
        (self.width_mm / 1000.0 * self.scale, self.height_mm / 1000.0 * self.scale)
    }

    /// Millimetres on the output per metre on the ground.
    pub fn mm_per_m(&self) -> f64 {
        1000.0 / self.scale
    }

    /// The ground under the output in lon/lat: its edge taken back through
    /// the projection. For UTM, or where that fails, a rectangle rotated by
    /// the bearing, its corners walked as geodesics from the centre, the
    /// sides the requested ground distance apart.
    pub fn query_polygon(&self) -> Polygon<f64> {
        if let Some(frame) = self.projected_frame() {
            return frame;
        }
        let g = Geodesic::wgs84();
        let (lon, lat) = self.center;
        let (w, h) = self.ground_size_m();
        let walk = |from: (f64, f64), azimuth: f64, dist: f64| -> (f64, f64) {
            let (lat2, lon2): (f64, f64) = g.direct(from.1, from.0, azimuth, dist);
            (lon2, lat2)
        };
        let b = self.bearing;
        let north = walk((lon, lat), b, h / 2.0);
        let south = walk((lon, lat), (b + 180.0) % 360.0, h / 2.0);
        let east = (b + 90.0) % 360.0;
        let west = (b + 270.0) % 360.0;
        let ne = walk(north, east, w / 2.0);
        let nw = walk(north, west, w / 2.0);
        let se = walk(south, east, w / 2.0);
        let sw = walk(south, west, w / 2.0);
        let ring: Vec<Coord<f64>> = [ne, nw, sw, se, ne].iter().map(|&(x, y)| Coord { x, y }).collect();
        Polygon::new(LineString::new(ring), vec![])
    }

    /// The output's edge taken back to the ground. A projection turns,
    /// shears or stretches the ground under the output, which a rectangle
    /// walked from the centre misses: web Mercator a continent wide reaches
    /// far further north than a geodesic walked east from the top's middle,
    /// which bends back toward the equator. Not UTM's: its distances are
    /// true, so the walked rectangle is its ground already.
    fn projected_frame(&self) -> Option<Polygon<f64>> {
        if self.projection == ProjectionMode::Utm {
            return None;
        }
        const PER_SIDE: usize = 16;
        let p = crate::projection::Projector::new(self);
        let (w, h) = (self.width_mm, self.height_mm);
        // A globe: the ground as far from the centre as the page's edge
        // reaches, the whole near side when the edge is off the globe.
        if let Some(globe) = p.globe() {
            let mut radius: f64 = 0.0;
            for i in 0..PER_SIDE * 4 {
                let t = (i % PER_SIDE) as f64 / PER_SIDE as f64;
                let (x, y) = match i / PER_SIDE {
                    0 => (w * t, 0.0),
                    1 => (w, h * t),
                    2 => (w * (1.0 - t), h),
                    _ => (0.0, h * (1.0 - t)),
                };
                radius = radius.max(p.unproject(x, y).map(|(lon, lat)| globe.distance(lon, lat) + 0.5).unwrap_or(90.0));
            }
            return Some(globe.ground(radius));
        }
        let edge = (0..PER_SIDE * 4).map(|i| {
            let t = (i % PER_SIDE) as f64 / PER_SIDE as f64;
            match i / PER_SIDE {
                0 => (w * t, 0.0),
                1 => (w, h * t),
                2 => (w * (1.0 - t), h),
                _ => (0.0, h * (1.0 - t)),
            }
        });
        let mut ring: Vec<Coord<f64>> =
            edge.map(|(x, y)| p.unproject(x, y).map(|(x, y)| Coord { x, y })).collect::<Option<_>>()?;
        // A pole on the page: the edge only circles it, so the ground is the
        // cap from the edge's farthest latitude to the pole, every longitude.
        if let Some(pole) = self.pole_on_page(&p) {
            let far = if pole > 0.0 { ring.iter().map(|c| c.y).fold(90.0, f64::min) } else { ring.iter().map(|c| c.y).fold(-90.0, f64::max) };
            let (a, b) = if pole > 0.0 { (far, 90.0) } else { (-90.0, far) };
            let cap = vec![
                Coord { x: -180.0, y: a },
                Coord { x: 180.0, y: a },
                Coord { x: 180.0, y: b },
                Coord { x: -180.0, y: b },
                Coord { x: -180.0, y: a },
            ];
            return Some(Polygon::new(LineString::new(cap), vec![]));
        }
        ring.push(ring[0]);
        Some(Polygon::new(LineString::new(ring), vec![]))
    }

    /// `90` or `-90` when that pole is drawn on the page, for a PROJ
    /// projection that reaches it (the web map's never does).
    pub fn pole_on_page(&self, p: &crate::projection::Projector) -> Option<f64> {
        // Web Mercator's poles are infinitely far: only its clamp at 85.05°
        // would put one on a page that reaches that far.
        if self.projection == ProjectionMode::Webmercator {
            return None;
        }
        [90.0, -90.0].into_iter().find(|&lat| {
            let (x, y) = p.project(0.0, lat);
            x.is_finite() && y.is_finite() && (0.0..=self.width_mm).contains(&x) && (0.0..=self.height_mm).contains(&y)
        })
    }

    /// `[west, south, east, north]` of the unrotated output area.
    pub fn bounds(&self) -> [f64; 4] {
        let g = Geodesic::wgs84();
        let (lon, lat) = self.center;
        let (w, h) = self.ground_size_m();
        let at = |azimuth: f64, dist: f64| -> (f64, f64) {
            let (lat2, lon2): (f64, f64) = g.direct(lat, lon, azimuth, dist);
            (lon2, lat2)
        };
        [at(270.0, w / 2.0).0, at(180.0, h / 2.0).1, at(90.0, w / 2.0).0, at(0.0, h / 2.0).1]
    }
}

/// OSM keys fetched for an output (each theme filters in memory afterwards).
pub const OSM_TAGS: &[&str] =
    &["building", "highway", "railway", "natural", "water", "landuse", "amenity", "leisure", "waterway"];

/// Keys to ask Overpass for with the older profile: the standard set, then
/// whatever else the profile selects on, since its themes read keys they
/// never declare. A grammar document asks for its own keys alone.
pub fn fetch_keys(needed: &[String]) -> Vec<String> {
    let mut keys: Vec<String> = OSM_TAGS.iter().map(|k| k.to_string()).collect();
    for k in needed {
        if !keys.contains(k) {
            keys.push(k.clone());
        }
    }
    keys
}

/// Overpass servers, tried in order. `z.`
/// and `lz4.` are overpass-api.de's own machines by name: when the main
/// address answers 504 one of them often still has a free slot. (The kumi
/// and private.coffee mirrors took connections and never answered.)
pub const OVERPASS_ENDPOINTS: &[&str] = &[
    "https://overpass-api.de/api/interpreter",
    "https://z.overpass-api.de/api/interpreter",
    "https://lz4.overpass-api.de/api/interpreter",
    "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
];

/// Seconds to wait before each round of tries over every endpoint. A busy
/// server's 504 often clears within a minute, so a failed round is worth
/// repeating a few times, further apart each time, before giving up.
pub const OVERPASS_ROUND_PAUSE_S: &[u64] = &[0, 10, 30, 60];

/// Longest wait for a free slot after a 429 that is worth sitting through;
/// beyond it the next server is tried instead.
pub const OVERPASS_MAX_SLOT_WAIT_S: u64 = 120;

/// The server's status page next to its `interpreter`: it says how many
/// query slots this address has free, or when the next one frees up.
pub fn overpass_status_url(interpreter: &str) -> String {
    match interpreter.strip_suffix("interpreter") {
        Some(base) => format!("{base}status"),
        None => format!("{}/status", interpreter.trim_end_matches('/')),
    }
}

/// Seconds until a query slot is free, from a status page: 0 when one is
/// free now, `None` when the page says neither.
pub fn overpass_slot_wait_s(status: &str) -> Option<u64> {
    if status.lines().any(|l| l.trim_end().ends_with("slots available now.") && !l.starts_with("0 ")) {
        return Some(0);
    }
    status
        .lines()
        .filter_map(|l| l.split(", in ").nth(1)?.strip_suffix(" seconds.")?.trim().parse::<u64>().ok())
        .min()
}

/// The error Overpass reports inside a 200 reply, when a query ran out of
/// time or memory and the elements are cut short: its `remark`, if it says
/// "error". Read from the end, where Overpass writes it.
pub fn overpass_error(json: &str) -> Option<String> {
    let mut start = json.len().saturating_sub(2000);
    while !json.is_char_boundary(start) {
        start += 1;
    }
    let tail = &json[start..];
    let at = tail.rfind("\"remark\"")?;
    let rest = tail[at + 8..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let text = rest.split('"').next()?.trim();
    text.contains("error").then(|| text.to_string())
}

/// Give up on one endpoint after this long, so a hanging server cannot stall
/// a draw.
pub const OVERPASS_ENDPOINT_TIMEOUT_S: u64 = 60;

/// The Overpass QL osmnx sends for `features_from_polygon(polygon, tags)`
/// with every tag set to `True`. Drawing asks for a padded box instead
/// (`osm_cache::request`); this one finds the replies osmnx cached, for the
/// parity examples.
pub fn overpass_query(polygon: &Polygon<f64>, tags: &[&str], timeout_s: u32) -> String {
    let coords: Vec<String> =
        polygon.exterior().coords().map(|c| format!("{:.6} {:.6}", c.y, c.x)).collect();
    let poly = coords.join(" ");
    let mut components = String::new();
    for key in tags {
        let tag_str = format!("['{key}'](poly:'{poly}');(._;>;);");
        for kind in ["node", "way", "relation"] {
            components.push_str(&format!("({kind}{tag_str});"));
        }
    }
    format!("[out:json][timeout:{timeout_s}];({components});out;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::BoundingRect;

    fn ferry() -> Output {
        Output {
            center: (-122.3934746, 37.7955487),
            width_mm: 152.4,
            height_mm: 101.6,
            scale: 10000.0,
            bearing: 0.0,
            projection: ProjectionMode::Utm,
            tile_zoom: None,
        }
    }

    #[test]
    fn query_matches_osmnx() {
        // Captured from osmnx 2.0.2 for the same output; the osmnx cache key
        // depends on this exact text.
        let q = overpass_query(&ferry().query_polygon(), OSM_TAGS, 180);
        assert!(q.starts_with(
            "[out:json][timeout:180];((node['building'](poly:'37.800125 -122.384822 37.800125 -122.402127 \
             37.790971 -122.402126 37.790971 -122.384823 37.800125 -122.384822');(._;>;););"
        ));
        assert!(q.ends_with(");out;"));
    }

    #[test]
    fn a_wide_web_mercator_page_reads_its_whole_ground() {
        // The 1-bit scene far out over the Mediterranean: 2205 × 1014 pixels
        // of 4.9 km. A geodesic walked east from the top's middle bends to
        // the equator, and the corners it found cut off the north.
        let scale = 4_859_118.0;
        let o = Output { center: (12.85, 41.5), width_mm: 2205.0, height_mm: 1014.0, scale, bearing: 0.0, projection: ProjectionMode::Webmercator, tile_zoom: None };
        let r = o.query_polygon().bounding_rect().unwrap();
        // The page's top and bottom in Mercator, back to latitudes.
        let a = 6_378_137.0;
        let y0 = a * (std::f64::consts::FRAC_PI_4 + 41.5f64.to_radians() / 2.0).tan().ln();
        let half = 507.0 * scale / 1000.0 / 41.5f64.to_radians().cos();
        let lat = |y: f64| (2.0 * (y / a).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
        assert!((r.max().y - lat(y0 + half)).abs() < 1e-6 && (r.min().y - lat(y0 - half)).abs() < 1e-6, "{r:?}");
        // Every corner's latitude is the edge's: the top is a parallel.
        let ring = o.query_polygon();
        let north = ring.exterior().0.iter().filter(|c| (c.y - lat(y0 + half)).abs() < 1e-6).count();
        assert!(north >= 17, "{ring:?}");
    }

    #[test]
    fn status_pages_and_remarks() {
        let busy = "Connected as: 1\nRate limit: 2\n0 slots available now.\n\
                    Slot available after: 2026-10-02T08:32:01Z, in 36 seconds.\n\
                    Slot available after: 2026-10-02T08:32:20Z, in 55 seconds.\n";
        let free = "Rate limit: 4\n3 slots available now.\nSlot available after: 2026-10-02T08:33:06Z, in 26 seconds.\n";
        assert_eq!(overpass_slot_wait_s(busy), Some(36));
        assert_eq!(overpass_slot_wait_s(free), Some(0));
        assert_eq!(overpass_slot_wait_s("<html>406</html>"), None);
        assert_eq!(
            overpass_status_url("https://overpass-api.de/api/interpreter"),
            "https://overpass-api.de/api/status"
        );
        let cut = r#"{"elements":[{"type":"node","id":1,"lat":0,"lon":0}],
"remark": "runtime error: Query timed out in \"query\" at line 1 after 61 seconds." }"#;
        assert!(overpass_error(cut).unwrap().starts_with("runtime error: Query timed out"));
        assert_eq!(overpass_error(r#"{"elements":[]}"#), None);
    }

    #[test]
    fn bounds_are_geodesic() {
        let [w, s, e, n] = ferry().bounds();
        assert!((n - 37.800125).abs() < 1e-6 && (s - 37.790971).abs() < 1e-6);
        assert!(w < -122.4021 && e > -122.3849);
    }
}
