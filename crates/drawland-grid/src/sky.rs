//! The sky past a globe's rim: the real stars, where they are behind the
//! Earth for the eye that looks at it. The eye looks down on `(lng, lat)`
//! from afar, north up, so behind the Earth is the sky's point opposite;
//! the stars are fixed in the sky as the Earth turns under them (sidereal
//! time), so turning the globe turns them too, the other way, as the far
//! side of anything one walks round does. The screen's shorter side spans
//! [`FIELD`] of sky, at any zoom: the stars are infinitely far, nearing the
//! Earth does not near them.
//!
//! A few hundred stars projected a frame: nothing beside the map.

use crate::stars::STARS;

/// Degrees of sky across the screen's shorter side.
pub const FIELD: f64 = 90.0;

/// The faintest star drawn.
pub const FAINTEST: f64 = 5.0;

/// The sky behind a globe seen over `(lng, lat)` at `unix` seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    pub lng: f64,
    pub lat: f64,
    pub unix: f64,
}

/// A star on the screen: where (units, from the top left) and how bright
/// (visual magnitude: lower is brighter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    pub x: f64,
    pub y: f64,
    pub magnitude: f64,
}

impl Sky {
    /// Greenwich mean sidereal time, degrees: how far the Earth has turned
    /// under the stars.
    pub fn sidereal(&self) -> f64 {
        let days = self.unix / 86_400.0 + 2_440_587.5 - 2_451_545.0;
        (280.460_618_37 + 360.985_647_366_29 * days).rem_euclid(360.0)
    }

    /// The stars on a screen `(w, h)` units (square units) wide and down.
    pub fn stars(&self, (w, h): (f64, f64)) -> Vec<Star> {
        let (l, p) = (self.lng.to_radians(), self.lat.to_radians());
        // The eye's frame, in the Earth's: it looks along `f` (from the
        // Earth's far side, away from the eye), east to its right, north up.
        let n = [p.cos() * l.cos(), p.cos() * l.sin(), p.sin()];
        let f = [-n[0], -n[1], -n[2]];
        let e = [-l.sin(), l.cos(), 0.0];
        let u = [-p.sin() * l.cos(), -p.sin() * l.sin(), p.cos()];
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let focal = w.min(h) / 2.0 / (FIELD / 2.0).to_radians().tan();
        let theta = self.sidereal();
        let (cx, cy) = (w / 2.0, h / 2.0);
        STARS
            .iter()
            .filter(|s| (s.2 as f64) <= FAINTEST)
            .filter_map(|&(ra, de, mag)| {
                // Its longitude over the Earth now.
                let (a, d) = ((ra as f64 - theta).to_radians(), (de as f64).to_radians());
                let s = [d.cos() * a.cos(), d.cos() * a.sin(), d.sin()];
                let depth = dot(s, f);
                if depth < 0.2 {
                    return None;
                }
                let (x, y) = (cx + focal * dot(s, e) / depth, cy - focal * dot(s, u) / depth);
                (x >= 0.0 && y >= 0.0 && x < w && y < h).then_some(Star { x, y, magnitude: mag as f64 })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behind_the_earth_is_the_sky_opposite() {
        // At the J2000 epoch's noon the sidereal time is 280.46°: over
        // longitude 0 the sky's 280.46° stands overhead, so behind the Earth
        // seen from there is right ascension 100.46°, declination 0, near
        // Sirius (101.29°, −16.7°): seen over the equator at longitude 0,
        // Sirius is near the middle across, and below it.
        let sky = Sky { lng: 0.0, lat: 0.0, unix: 946_728_000.0 };
        assert!((sky.sidereal() - 280.46).abs() < 0.01);
        let stars = sky.stars((1000.0, 1000.0));
        let sirius = stars.iter().min_by(|a, b| a.magnitude.total_cmp(&b.magnitude)).unwrap();
        assert!(sirius.magnitude < -1.0);
        assert!(sirius.y > 600.0 && (sirius.x - 500.0).abs() < 40.0, "{sirius:?}");
    }

    #[test]
    fn turning_the_globe_turns_the_stars_the_other_way() {
        let at = |lng: f64| {
            let stars = Sky { lng, lat: 0.0, unix: 946_728_000.0 }.stars((1000.0, 1000.0));
            stars.into_iter().min_by(|a, b| a.magnitude.total_cmp(&b.magnitude)).unwrap()
        };
        // Turned east, the land comes from the right; the stars go right.
        assert!(at(5.0).x > at(0.0).x + 20.0);
    }
}
