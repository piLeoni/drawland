//! The Rust face (drawland-graph's typed API) builds the graph a JavaScript
//! profile builds, node for node.

use drawland_graph::{obj, Arg, Graph};

#[test]
fn rust_builds_what_javascript_builds() {
    let js = r#"
title("Lake Merritt")
output({ lat: 37.802, lng: -122.263, width: "6in", height: "4in", scale: "1:8000" })
const buildings = areas.polygons(features.tiles("buildings"))
const water = areas.polygons(features.tiles("water"))
const roads = modules.roads({ from: features.tiles("roads", "paths") })
draw({ id: "water", from: fills.fill({ in: water, pattern: "waves", spacing: "0.8mm" }) })
draw({ id: "roads", from: roads.outlines })
draw({ id: "buildings", from: lines.rings(buildings) })
compose.mask({ from: roads.area, occludes: ["water"] })
compose.simplify(["roads"], { tolerance: 0.05 })
"#;
    let from_js = drawland_script::evaluate(js).unwrap();

    let mut g = Graph::new(None);
    g.title("Lake Merritt").unwrap();
    g.output(obj! { "lat" => 37.802, "lng" => -122.263, "width" => "6in", "height" => "4in", "scale" => "1:8000" }).unwrap();
    let b = g.features().tiles(&["buildings"]).unwrap();
    let buildings = g.areas().polygons(&b).unwrap();
    let w = g.features().tiles(&["water"]).unwrap();
    let water = g.areas().polygons(&w).unwrap();
    let r = g.features().tiles(&["roads", "paths"]).unwrap();
    let roads = g.modules().roads(obj! { "from" => &r }).unwrap();
    let waves = g.fills().fill(obj! { "in" => &water, "pattern" => "waves", "spacing" => "0.8mm" }).unwrap();
    g.draw(obj! { "id" => "water", "from" => &waves }).unwrap();
    g.draw(obj! { "id" => "roads", "from" => &roads.outlines }).unwrap();
    let rings = g.lines().rings(&buildings, obj! {}).unwrap();
    g.draw(obj! { "id" => "buildings", "from" => &rings }).unwrap();
    g.compose().mask(obj! { "from" => &roads.area, "occludes" => vec!["water"] }).unwrap();
    g.compose().step("simplify", &["roads"], obj! { "tolerance" => 0.05 }).unwrap();
    let from_rust = g.finish().unwrap();

    // Positions differ (the Rust calls have none); everything else is equal.
    let strip = |mut v: serde_json::Value| {
        fn walk(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(o) => {
                    o.remove("at");
                    o.values_mut().for_each(walk);
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(walk),
                _ => {}
            }
        }
        walk(&mut v);
        v
    };
    assert_eq!(strip(from_rust.to_json()), strip(from_js.to_json()));
}

#[test]
fn the_same_checks_and_messages() {
    let mut g = Graph::new(None);
    let e = g.call("areas.union", &[Arg::Num(1.0)]).unwrap_err();
    assert_eq!(e.message, "areas.union expects an area (from areas.*, page.frame or a module's .area), got the number 1");
    let e = g.call("features.contours", &[Arg::Map(obj! { "step" => 50, "spacing" => 1 })]).unwrap_err();
    assert!(e.message.contains("unknown key spacing"), "{e}");
}

#[test]
fn views_and_remaps_take_rust_functions() {
    let mut g = Graph::new(None);
    g.output(obj! { "lat" => 45.0, "lng" => 9.0, "scale" => "1:20000", "warp" => vec![Arg::Map(obj! { "remap" => true, "grid" => 4 })] })
        .unwrap_err();
    let mut g = Graph::new(None);
    let mut mirror = |g: &mut Graph, [x, y]: [f64; 2]| -> drawland_graph::Result<[f64; 2]> {
        let w = g.page().width()?;
        Ok([w - x, y])
    };
    g.output_remap(
        obj! { "lat" => 45.0, "lng" => 9.0, "scale" => "1:20000", "warp" => vec![Arg::Map(obj! { "remap" => true, "grid" => 4 })] },
        &mut [&mut mirror],
    )
    .unwrap();
    let id = g
        .view(obj! { "id" => "inset", "at" => [0, 0], "size" => [40, 30], "lat" => 41.4, "lng" => 2.17, "scale" => "1:4000" }, |g| {
            let t = g.features().tiles(&["roads"])?;
            let l = g.lines().of(&t)?;
            g.draw(obj! { "id" => "roads", "from" => &l }).map(|_| ())
        })
        .unwrap();
    assert_eq!(id, "inset");
    let graph = g.finish().unwrap();
    assert_eq!(graph.views.len(), 1);
    assert_eq!(graph.output.warp.0.len(), 1, "the remap became a mesh");
}
