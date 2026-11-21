//! A moving screen zoomed far out becomes a globe, as draw.land's page
//! does: past [`FAR_KM`] of ground across the screen's shorter side, the
//! graph is drawn in the orthographic projection round the view's centre
//! (`drawland_core::ortho`), and what lies past the globe's rim is cleared
//! ([`drawland_grid::Picture::globe`]). Nearer, web Mercator and the moving
//! scene as before; at that width the two differ little in the middle.

use drawland_core::grammar::Grammar;
use drawland_core::ortho::RADIUS;
use drawland_core::profile::ProjectionMode;
use drawland_grid::View;

/// Ground across the screen's shorter side, km, past which it is a globe.
pub const FAR_KM: f64 = 4000.0;

/// The ground a unit spans, mm (true at the centre), from a view.
pub fn scale_of(view: &View) -> f64 {
    let (_, lat) = drawland_grid::scene::geographic(view.center);
    view.per_unit * 1000.0 * lat.to_radians().cos()
}

/// Whether a screen `units` across and down, a unit spanning `scale` mm,
/// is drawn as a globe.
pub fn far(scale: f64, (w, h): (f64, f64)) -> bool {
    scale / 1e6 * w.min(h) > FAR_KM
}

/// Whether `view` is drawn as a globe.
pub fn far_view(view: &View) -> bool {
    far(scale_of(view), view.screen.size())
}

/// The graph's output made a globe round its centre.
pub fn make(grammar: &mut Grammar) {
    grammar.output.projection = ProjectionMode::Proj("+proj=ortho +lat_0={lat} +lon_0={lng}".into());
}

/// The globe's radius in units, a unit spanning `scale` mm.
pub fn radius(scale: f64) -> f64 {
    RADIUS / (scale / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use drawland_core::paint::{Screen, ScreenKind};

    #[test]
    fn far_out_is_a_globe() {
        let screen = Screen { kind: ScreenKind::Bitmap, width: 800, height: 480, invert: false };
        assert!(!far_view(&View::at(12.0, 45.0, 3000.0, screen)), "a city");
        assert!(far_view(&View::at(12.0, 45.0, 20_000_000.0, screen)), "20 km a pixel: 9600 km");
        // 100 km a pixel: the globe is 128 pixels round.
        assert!((radius(100_000_000.0) - 63.78).abs() < 0.01);
    }
}
