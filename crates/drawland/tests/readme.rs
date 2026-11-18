//! The README's Rust example, kept compiling (drawing it needs the network,
//! so it stops at the profile).

use drawland::{Drawer, Place, Profile, Settings};
use drawland_graph::{obj, Graph};

#[test]
fn the_readme_example_builds() -> Result<(), Box<dyn std::error::Error>> {
    let mut g = Graph::new(None);
    g.output(obj! { "lat" => 37.802, "lng" => -122.263, "width" => "6in", "height" => "4in", "scale" => "1:8000" })?;
    let w = g.features().tiles(&["water"])?;
    let water = g.areas().polygons(&w)?;
    let waves = g.fills().fill(obj! { "in" => &water, "pattern" => "waves", "spacing" => "0.8mm" })?;
    g.draw(obj! { "id" => "water", "from" => &waves })?;
    let profile = Profile::graph(g.finish()?);
    let drawer = Drawer::new(Settings { offline: true, ..Settings::default() });
    let graph = drawer.graph(&profile, &Place::default(), &())?;
    assert_eq!(graph.draw.len(), 1);
    Ok(())
}
