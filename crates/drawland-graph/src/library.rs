//! The library a profile calls, by name: `features.tiles`, `areas.union`,
//! `draw`, `compose.mask`… Every check, default and message is here, once,
//! for JavaScript, Python and Rust alike. What each call adds to the graph is
//! the operation as the graph's JSON writes it.

use crate::arg::{describe, show_number, Arg, Lazy, Obj};
use crate::graph::{Graph, Result};
use drawland_core::grammar::{Clip, Section};
use serde_json::{json, Map, Value};
use std::f64::consts::PI;

/// Every function of the library, as a script names it: what `drawland.d.ts`
/// declares and the Python package exposes.
pub const FUNCTIONS: &[&str] = &[
    "areas.around",
    "areas.band",
    "areas.buffer",
    "areas.difference",
    "areas.extendToFrame",
    "areas.fillHoles",
    "areas.grow",
    "areas.intersect",
    "areas.move",
    "areas.polygons",
    "areas.shape",
    "areas.text",
    "areas.union",
    "burin.script",
    "burin.terrain",
    "compose.clip",
    "compose.deoverlap",
    "compose.filter",
    "compose.hand",
    "compose.linemerge",
    "compose.linesort",
    "compose.mask",
    "compose.order",
    "compose.orient",
    "compose.reloop",
    "compose.simplify",
    "draw",
    "engine.ocean",
    "features.contours",
    "features.geojson",
    "features.json",
    "features.osm",
    "features.soundings",
    "features.tiles",
    "features.where",
    "fills.cover",
    "fills.fill",
    "ink.at",
    "ink.ramp",
    "ink.width",
    "lines.clip",
    "lines.contours",
    "lines.dedupe",
    "lines.join",
    "lines.merge",
    "lines.of",
    "lines.rings",
    "lines.segments",
    "lines.shape",
    "lines.terrain",
    "lines.text",
    "modules.graticule",
    "modules.labels",
    "modules.roads",
    "output",
    "paint",
    "page.toWorld",
    "title",
    "view",
    "world.toPage",
];

/// The values a script reads rather than calls: `page.width`, `world.center`…
pub const PROPERTIES: &[&str] = &[
    "page.center",
    "page.frame",
    "page.height",
    "page.width",
    "world.bearing",
    "world.bounds",
    "world.center",
    "world.projection",
    "world.scale",
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Features,
    Area,
    Lines,
}

impl Kind {
    fn wants(self) -> &'static str {
        match self {
            Kind::Features => "features (from features.osm or features.tiles)",
            Kind::Area => "an area (from areas.*, page.frame or a module's .area)",
            Kind::Lines => "lines (from lines.*, fills.fill or a module)",
        }
    }
}

fn is(v: Option<&Arg>, kind: Kind) -> bool {
    matches!(
        (v, kind),
        (Some(Arg::Features(_)), Kind::Features) | (Some(Arg::Area { .. }), Kind::Area) | (Some(Arg::Lines { .. }), Kind::Lines)
    )
}

/// The argument at `i`, `None` past the end.
fn at(args: &[Arg], i: usize) -> Option<&Arg> {
    args.get(i)
}

/// An optional argument: `None` past the end or when it is `undefined`-like
/// (the caller left it out).
fn opt(args: &[Arg], i: usize) -> Option<&Arg> {
    args.get(i)
}

impl Graph {
    fn fail<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(self.error(message))
    }

    /// The graph reference of `v`, checked to be a `kind`.
    fn need(&self, v: Option<&Arg>, kind: Kind, wh: &str) -> Result<String> {
        if !is(v, kind) {
            return self.fail(format!("{wh} expects {}, got {}", kind.wants(), describe(v)));
        }
        match v.expect("checked") {
            Arg::Lines { r, module: Some(m) } => self.fail(format!("{wh}: {r} comes from module {m}; it can be drawn, not transformed")),
            Arg::Features(r) | Arg::Area { r, .. } | Arg::Lines { r, .. } => Ok(r.clone()),
            _ => unreachable!(),
        }
    }

    /// `f(a, b, c)` or `f([a, b, c])`.
    fn many(&self, args: &[Arg], kind: Kind, wh: &str) -> Result<Vec<String>> {
        let items: &[Arg] = match args {
            [Arg::List(l)] => l,
            _ => args,
        };
        let n = items.len();
        items
            .iter()
            .enumerate()
            .map(|(i, x)| self.need(Some(x), kind, &if n > 1 { format!("{wh} (input {})", i + 1) } else { wh.to_string() }))
            .collect()
    }

    fn object<'a>(&self, v: Option<&'a Arg>, wh: &str) -> Result<&'a Obj> {
        match v {
            Some(Arg::Map(o)) => Ok(o),
            other => self.fail(format!("{wh} expects an object, got {}", describe(other))),
        }
    }

    /// An options object that may be left out: `{}` then.
    fn options(&self, v: Option<&Arg>, wh: &str) -> Result<Obj> {
        match v {
            None => Ok(Obj::new()),
            v => self.object(v, wh).cloned(),
        }
    }

    /// A length in millimetres: `0.3` or `"0.3mm"`.
    fn mm(&self, v: Option<&Arg>, wh: &str) -> Result<f64> {
        let n = match v {
            Some(Arg::Num(n)) => *n,
            Some(Arg::Str(s)) => parse_mm(s).unwrap_or(f64::NAN),
            _ => f64::NAN,
        };
        if !n.is_finite() {
            return self.fail(format!("{wh} is a length in millimetres like 0.3 or \"0.3mm\", got {}", describe(v)));
        }
        Ok(n)
    }

    fn make(&mut self, section: Section, op: Value) -> Result<String> {
        self.node(section, op)
    }

    fn new_features(&mut self, op: Value) -> Result<Arg> {
        Ok(Arg::Features(self.make(Section::Features, op)?))
    }

    fn area(&mut self, op: Value) -> Result<Arg> {
        let r = self.make(Section::Areas, op)?;
        Ok(Arg::Area { mask: r.clone(), r })
    }

    fn lines_node(&mut self, section: Section, op: Value) -> Result<Arg> {
        Ok(Arg::Lines { r: self.make(section, op)?, module: None })
    }

    /// Call the library's function `name` (`"areas.union"`) with `args`.
    pub fn call(&mut self, name: &str, args: &[Arg]) -> Result<Arg> {
        // What a caller left off the end is not there.
        let mut n = args.len();
        while n > 0 && args[n - 1] == Arg::Null {
            n -= 1;
        }
        let args = &args[..n];
        match name {
            "features.osm" => {
                let tags = self.object(at(args, 0), "features.osm")?.clone();
                self.new_features(json!({ "osm_select": Arg::Map(tags).to_json() }))
            }
            "features.tiles" => {
                let flat: Vec<Arg> = args.iter().flat_map(|a| match a {
                    Arg::List(l) => l.clone(),
                    other => vec![other.clone()],
                }).collect();
                if flat.is_empty() {
                    return self.fail("features.tiles needs a preset or a layer:class rule");
                }
                self.new_features(json!({ "tiles_select": Arg::List(flat).to_json() }))
            }
            "features.json" => self.features_json(args),
            "features.contours" => self.features_contours(args),
            "features.soundings" => self.features_soundings(args),
            "features.geojson" => {
                match at(args, 0) {
                    Some(Arg::Map(o)) => {
                        if o.get("type").and_then(Arg::as_str).is_none() {
                            return self.fail("features.geojson: an inline object is GeoJSON, with a type");
                        }
                        let o = o.clone();
                        self.new_features(json!({ "data_select": { "geojson": Arg::Map(o).to_json() } }))
                    }
                    Some(Arg::Str(s)) if !s.trim().is_empty() => {
                        let s = s.trim().to_string();
                        self.new_features(json!({ "data_select": s }))
                    }
                    other => self.fail(format!("features.geojson takes a URL, a file path, or a GeoJSON object, got {}", describe(other))),
                }
            }
            "features.where" => self.features_where(args),

            "areas.band" => self.areas_band(args),
            "areas.polygons" => {
                let r = self.need(at(args, 0), Kind::Features, "areas.polygons")?;
                self.area(json!({ "tools_polygons": r }))
            }
            "areas.union" => {
                let refs = self.many(args, Kind::Area, "areas.union")?;
                if refs.is_empty() {
                    return self.fail("areas.union needs at least one area");
                }
                self.area(json!({ "tools_union": refs }))
            }
            "areas.difference" => {
                let b = self.need(at(args, 0), Kind::Area, "areas.difference")?;
                let refs = self.many(&args[1..], Kind::Area, "areas.difference cut")?;
                if refs.is_empty() {
                    return Ok(args[0].clone());
                }
                self.area(json!({ "tools_difference": { "base": b, "cut": refs } }))
            }
            "areas.intersect" => {
                let base = self.need(at(args, 0), Kind::Area, "areas.intersect")?;
                let keep = self.need(at(args, 1), Kind::Area, "areas.intersect keep")?;
                self.area(json!({ "tools_intersect": { "base": base, "keep": keep } }))
            }
            "areas.fillHoles" => {
                let r = self.need(at(args, 0), Kind::Area, "areas.fillHoles")?;
                self.area(json!({ "tools_fill_holes": r }))
            }
            "areas.move" => {
                let by = match at(args, 1) {
                    Some(Arg::List(l)) if l.len() == 2 => Arg::List(l.clone()),
                    other => return self.fail(format!("areas.move: by is [across, down], like [\"2mm\", \"1mm\"], got {}", describe(other))),
                };
                let of = self.need(at(args, 0), Kind::Area, "areas.move")?;
                self.area(json!({ "tools_move": { "of": of, "by": by.to_json() } }))
            }
            "areas.grow" => {
                let by = self.length_arg(at(args, 1), "areas.grow: by is a length like \"0.5mm\"")?;
                let of = self.need(at(args, 0), Kind::Area, "areas.grow")?;
                self.area(json!({ "tools_grow": { "of": of, "by": by } }))
            }
            "areas.around" => {
                let by = self.length_arg(at(args, 1), "areas.around: by is a length like \"2mm\"")?;
                let of = self.need(at(args, 0), Kind::Features, "areas.around")?;
                self.area(json!({ "tools_around": { "of": of, "by": by } }))
            }
            "areas.shape" => {
                let op = self.shape_op(at(args, 0), "areas.shape")?;
                self.area(json!({ "tools_shape": op }))
            }
            "areas.text" => {
                let op = self.text_op(at(args, 0), "areas.text")?;
                self.area(json!({ "tools_text_area": op }))
            }
            "areas.buffer" => {
                let options = self.options(opt(args, 2), "areas.buffer options")?;
                let by = self.length_arg(at(args, 1), "areas.buffer: by is a length like \"0.2mm\"")?;
                let lines = self.need(at(args, 0), Kind::Lines, "areas.buffer")?;
                let mut op = json!({ "lines": lines, "by": by });
                if let Some(cap) = options.get("cap") {
                    match cap.as_str() {
                        Some("round") => {}
                        Some("flat") => {
                            op["cap"] = json!("flat");
                        }
                        _ => return self.fail(format!("areas.buffer: cap is \"round\" or \"flat\", got {}", describe(Some(cap)))),
                    }
                }
                self.area(json!({ "tools_buffer_lines": op }))
            }
            "areas.extendToFrame" => {
                let options = self.options(opt(args, 2), "areas.extendToFrame options")?;
                let of = self.need(at(args, 0), Kind::Area, "areas.extendToFrame")?;
                let within = self.need(at(args, 1), Kind::Area, "areas.extendToFrame within")?;
                let mut op = json!({ "of": of, "within": within });
                if let Some(g) = options.get("gapMm").filter(|g| **g != Arg::Null) {
                    op["gap_mm"] = g.to_json();
                }
                self.area(json!({ "tools_extend_to_frame": op }))
            }

            "engine.ocean" => {
                let frame = self.get_property("page.frame")?;
                let tiles = self.call("features.tiles", &[Arg::str("ocean")])?;
                let polys = self.call("areas.polygons", &[tiles])?;
                let raw = self.call("areas.intersect", &[polys, frame.clone()])?;
                let grown = self.call("areas.grow", &[raw, Arg::str("0.35mm")])?;
                self.call("areas.extendToFrame", &[grown, frame])
            }

            "page.toWorld" => {
                let (x, y) = self.page_point(at(args, 0), "page.toWorld")?;
                let f = self.frame("page.toWorld")?;
                match f.to_world(x, y) {
                    Some((lng, lat)) => Ok(Arg::Map(crate::obj! { "lat" => lat, "lng" => lng })),
                    None => self.fail(format!("page.toWorld: [{}, {}] is off the edge of the projection's world", show_number(x), show_number(y))),
                }
            }
            "world.toPage" => {
                let (lat, lng) = self.ground_point(at(args, 0), "world.toPage")?;
                let f = self.frame("world.toPage")?;
                let (x, y) = f.to_page(lng, lat);
                Ok(Arg::List(vec![Arg::Num(x), Arg::Num(y)]))
            }

            "lines.rings" => {
                let options = self.options(opt(args, 1), "lines.rings options")?;
                let of = self.need(at(args, 0), Kind::Area, "lines.rings")?;
                let ext = options.get("exteriorOnly").is_some_and(Arg::truthy);
                self.lines_node(Section::Lines, json!({ "tools_rings": { "of": of, "exterior_only": ext } }))
            }
            "lines.of" => {
                let r = self.need(at(args, 0), Kind::Features, "lines.of")?;
                self.lines_node(Section::Lines, json!({ "tools_lines": r }))
            }
            "lines.shape" => {
                let mut op = self.shape_op(at(args, 0), "lines.shape")?;
                let spec = at(args, 0).and_then(Arg::as_map).expect("shape_op checked it");
                let closed = matches!(spec.get("points"), None | Some(Arg::Null)) || spec.get("closed").is_some_and(Arg::truthy);
                op["closed"] = Value::Bool(closed);
                self.lines_node(Section::Lines, json!({ "tools_path": op }))
            }
            "lines.text" => {
                let op = self.text_op(at(args, 0), "lines.text")?;
                self.lines_node(Section::Lines, json!({ "tools_text": op }))
            }
            "lines.segments" => {
                let r = self.need(at(args, 0), Kind::Features, "lines.segments")?;
                self.lines_node(Section::Lines, json!({ "tools_segments": r }))
            }
            "lines.dedupe" => {
                let r = self.need(at(args, 0), Kind::Lines, "lines.dedupe")?;
                self.lines_node(Section::Lines, json!({ "tools_dedupe": r }))
            }
            "lines.merge" => {
                let r = self.need(at(args, 0), Kind::Lines, "lines.merge")?;
                self.lines_node(Section::Lines, json!({ "tools_merge": r }))
            }
            "lines.join" => {
                let refs = self.many(args, Kind::Lines, "lines.join")?;
                if refs.is_empty() {
                    return self.fail("lines.join needs at least one set of lines");
                }
                self.lines_node(Section::Lines, json!({ "tools_join": refs }))
            }
            "lines.clip" => {
                let options = self.options(opt(args, 2), "lines.clip options")?;
                let lines = self.need(at(args, 0), Kind::Lines, "lines.clip")?;
                let within = self.need(at(args, 1), Kind::Area, "lines.clip within")?;
                let invert = options.get("invert").is_some_and(Arg::truthy);
                self.lines_node(Section::Lines, json!({ "tools_clip": { "lines": lines, "within": within, "invert": invert } }))
            }
            "lines.terrain" => {
                let options = self.options(opt(args, 0), "lines.terrain options")?;
                self.lines_terrain(options)
            }
            "lines.contours" => {
                let options = self.options(opt(args, 0), "lines.contours options")?;
                let mut rest = options.without(&["from", "to"]);
                let mut wh = match rest.get("where") {
                    Some(Arg::Map(w)) => w.clone(),
                    _ => Obj::new(),
                };
                if let Some(v) = options.get("from") {
                    wh.set("elevMin", v.clone());
                }
                if let Some(v) = options.get("to") {
                    wh.set("elevMax", v.clone());
                }
                rest.set("where", Arg::Map(wh));
                rest.set("draw", Arg::str("fill"));
                self.lines_terrain(rest)
            }

            "burin.terrain" | "burin.script" => self.burin_terrain(args),

            "fills.fill" => {
                let spec = self.object(at(args, 0), "fills.fill")?.clone();
                let area = self.need(spec.get("in"), Kind::Area, "fills.fill in")?;
                let mut op = Map::new();
                op.insert("in".into(), Value::String(area));
                for (k, v) in spec.without(&["in", "per", "content"]).0 {
                    op.insert(k, v.to_json());
                }
                if let Some(per) = spec.get("per") {
                    op.insert("per".into(), per.to_json());
                }
                if let Some(content) = spec.get("content") {
                    op.insert("content".into(), Value::String(self.need(Some(content), Kind::Lines, "fills.fill content")?));
                }
                if let Some(m @ Arg::Lines { .. }) = spec.get("motif") {
                    op.insert("motif".into(), Value::String(self.need(Some(m), Kind::Lines, "fills.fill motif")?));
                }
                self.lines_node(Section::Fills, json!({ "tools_fill": Value::Object(op) }))
            }
            "fills.cover" => {
                let spec = self.object(at(args, 0), "fills.cover")?.clone();
                if spec.has("per") || spec.has("content") {
                    return self.fail("fills.cover: takes a pattern, not per or content");
                }
                let area = self.need(spec.get("in"), Kind::Area, "fills.cover in")?;
                let mut op = Map::new();
                op.insert("in".into(), Value::String(area));
                for (k, v) in spec.without(&["in"]).0 {
                    op.insert(k, v.to_json());
                }
                if let Some(m @ Arg::Lines { .. }) = spec.get("motif") {
                    op.insert("motif".into(), Value::String(self.need(Some(m), Kind::Lines, "fills.cover motif")?));
                }
                self.area(json!({ "tools_fill_cover": Value::Object(op) }))
            }

            "ink.ramp" => self.ink_ramp(args),
            "ink.width" => {
                let a = self.mm(at(args, 0), "ink.width thin")?;
                let b = self.mm(at(args, 1), "ink.width thick")?;
                let t = self.unit(at(args, 2), "ink.width")?;
                let w = a + (b - a) * t;
                Ok(Arg::Str(format!("{}mm", show_number((w * 1000.0).round() / 1000.0))))
            }
            "ink.at" => {
                let nums: Vec<Option<f64>> = (0..3).map(|i| at(args, i).and_then(Arg::as_finite)).collect();
                match nums[..] {
                    [Some(v), Some(lo), Some(hi)] if lo != hi => Ok(Arg::Num(((v - lo) / (hi - lo)).clamp(0.0, 1.0))),
                    _ => self.fail("ink.at takes three numbers, the value and two different ends"),
                }
            }

            "modules.roads" => self.modules_roads(args),
            "modules.graticule" => self.modules_graticule(args),
            "modules.labels" => self.modules_labels(args),

            "draw" => {
                let spec = self.object(at(args, 0), "draw")?.clone();
                if let Some(extra) = spec.without(&["id", "from", "stroke"]).keys().next() {
                    return self.fail(format!("draw: unknown key {extra} (id, from, stroke)"));
                }
                let from = match spec.get("from") {
                    Some(Arg::Lines { r, .. }) => r.clone(),
                    other => return self.fail(format!("draw: from expects {}, got {}", Kind::Lines.wants(), describe(other))),
                };
                let id = spec.get("id").map(stringify);
                Ok(Arg::Str(self.add_draw(&from, id, spec.get("stroke"))?))
            }

            "paint" => self.paint(args),

            "compose.order" => {
                let items: &[Arg] = match args {
                    [Arg::List(l)] => l,
                    _ => args,
                };
                self.set_order(items.iter().map(stringify).collect());
                Ok(Arg::Null)
            }
            "compose.mask" => {
                let spec = self.object(at(args, 0), "compose.mask")?.clone();
                let from = match spec.get("from") {
                    Some(Arg::Area { mask, .. }) => mask.clone(),
                    other => return self.fail(format!("compose.mask: from expects {}, got {}", Kind::Area.wants(), describe(other))),
                };
                let occludes = match spec.get("occludes") {
                    None => Vec::new(),
                    Some(Arg::List(l)) => l.iter().map(stringify).collect(),
                    other => return self.fail(format!("compose.mask: occludes is a list of draw ids, got {}", describe(other))),
                };
                let name = match spec.get("name") {
                    None | Some(Arg::Null) => None,
                    Some(n) => Some(stringify(n)),
                };
                self.add_mask(name, &from, occludes)?;
                Ok(Arg::Null)
            }
            "compose.clip" => {
                let clip = match at(args, 0) {
                    Some(Arg::Str(s)) if s == "auto" => Clip::Auto,
                    Some(Arg::Str(s)) if s == "none" => Clip::None,
                    Some(Arg::List(l)) => {
                        let refs = l.iter().map(|a| self.need(Some(a), Kind::Area, "compose.clip")).collect::<Result<Vec<_>>>()?;
                        self.clip_areas(&refs)?
                    }
                    other => return self.fail(format!("compose.clip expects \"auto\", \"none\" or a list of areas, got {}", describe(other))),
                };
                self.set_clip(clip);
                Ok(Arg::Null)
            }
            "compose.deoverlap" => self.optimize(args, "deoverlap", Some("tolerance"), &[
                ("tolerance", "tolerance"),
                ("selfOverlap", "self_overlap"),
                ("acrossLayers", "across_layers"),
                ("angle", "angle"),
                ("prefer", "prefer"),
                ("minLength", "min_length"),
                ("drop", "drop"),
            ], is_length_value),
            "compose.linemerge" => self.optimize(args, "linemerge", Some("tolerance"), &[("tolerance", "tolerance")], is_length_value),
            "compose.simplify" => self.optimize(args, "simplify", Some("tolerance"), &[("tolerance", "tolerance")], is_length_value),
            "compose.filter" => self.optimize(args, "filter", Some("minLength"), &[("minLength", "min_length")], is_length_value),
            "compose.reloop" => self.optimize(args, "reloop", None, &[], is_length_value),
            "compose.hand" => self.optimize(args, "hand", Some("preset"), &[
                ("preset", "preset"),
                ("wobble", "wobble"),
                ("wavelength", "wavelength"),
                ("tremor", "tremor"),
                ("bow", "bow"),
                ("overshoot", "overshoot"),
                ("close", "close"),
                ("reach", "reach"),
                ("corners", "corners"),
                ("retrace", "retrace"),
                ("retraceOffset", "retrace_offset"),
                ("seed", "seed"),
            ], |v| matches!(v, Arg::Str(s) if s == "steady" || s == "sketch")),
            "compose.orient" => self.optimize(args, "orient", Some("direction"), &[
                ("direction", "direction"),
                ("angle", "direction"),
                ("maxLength", "max_length"),
                ("splitReversals", "split_reversals"),
            ], is_direction_value),
            "compose.linesort" => self.optimize(args, "linesort", Some("direction"), &[
                ("twoOpt", "two_opt"),
                ("passes", "passes"),
                ("follow", "follow"),
                ("direction", "direction"),
                ("angle", "direction"),
                ("maxLength", "max_length"),
                ("splitReversals", "split_reversals"),
            ], is_direction_value),

            "output" => {
                let spec = self.object(at(args, 0), "output")?.clone();
                self.set_plain_output(&spec)?;
                Ok(Arg::Null)
            }
            "title" => match at(args, 0) {
                Some(Arg::Str(s)) => {
                    let s = s.clone();
                    self.set_title(s);
                    Ok(Arg::Null)
                }
                other => self.fail(format!("title expects a string, got {}", describe(other))),
            },
            "view" => self.fail("view takes a style: call it from a script, or use Graph::view_begin / view_end"),
            other => match other.strip_prefix("page.").or_else(|| other.strip_prefix("world.")) {
                Some(_) if PROPERTIES.contains(&other) => self.get_property(other),
                _ => self.fail(format!("{other} is not a function of the library")),
            },
        }
    }

    /// The value of `page.width`, `world.center` and the like.
    pub fn get_property(&mut self, name: &str) -> Result<Arg> {
        match name {
            "page.width" => Ok(Arg::Num(self.current_output("page.width")?.width.0)),
            "page.height" => Ok(Arg::Num(self.current_output("page.height")?.height.0)),
            "page.center" => {
                let o = self.current_output("page.center")?;
                Ok(Arg::List(vec![Arg::Num(o.width.0 / 2.0), Arg::Num(o.height.0 / 2.0)]))
            }
            "page.frame" => self.area(json!({ "engine_frame": {} })),
            "world.scale" => Ok(Arg::Num(self.current_output("world.scale")?.scale.0)),
            "world.bearing" => Ok(Arg::Num(self.current_output("world.bearing")?.bearing)),
            "world.projection" => {
                let p = self.current_output("world.projection")?.projection;
                Ok(Arg::from_json(&serde_json::to_value(p).unwrap_or(Value::Null)))
            }
            "world.center" => {
                let f = self.frame("world.center")?;
                Ok(Arg::Map(crate::obj! { "lat" => f.output.center.1, "lng" => f.output.center.0 }))
            }
            "world.bounds" => {
                let f = self.frame("world.bounds")?;
                let b = f.bounds();
                Ok(Arg::Map(crate::obj! { "north" => b.north, "south" => b.south, "east" => b.east, "west" => b.west }))
            }
            other => self.fail(format!("{other} is not a property of the library")),
        }
    }

    /// A module's part made when it is first asked for: `graticule.keys`,
    /// `labels.area`, a label rule's `lines`. The same part every time.
    pub fn get(&mut self, of: &Arg, part: &str) -> Result<Arg> {
        let Arg::Lazy(lazy) = of else {
            return self.fail(format!("{} has no part {part}", describe(Some(of))));
        };
        let key = format!("{lazy:?}:{part}");
        if let Some(found) = self.top_ref().memo.get(&key) {
            return Ok(found.clone());
        }
        let made = match (lazy, part) {
            (Lazy::Graticule(look), "lines") => self.graticule_part(look, Section::Lines, "grid")?,
            (Lazy::Graticule(look), "keys") => self.graticule_part(look, Section::Areas, "keys")?,
            (Lazy::Graticule(look), "labels") => self.graticule_part(look, Section::Lines, "labels")?,
            (Lazy::Graticule(look), "labelArea") => self.graticule_part(look, Section::Areas, "labels")?,
            (Lazy::Labels { r, .. }, "lines") => Arg::Lines { r: r.clone(), module: None },
            (Lazy::Labels { r, .. }, "area") => self.area(json!({ "tools_label_area": r }))?,
            (Lazy::Labels { r, .. }, "dots") => self.area(json!({ "tools_label_area": { "of": r, "dots": true } }))?,
            (Lazy::Labels { r, rules }, id) if rules.iter().any(|x| x == id) => Arg::Lazy(Lazy::LabelRule { r: r.clone(), rule: id.to_string() }),
            (Lazy::LabelRule { r, rule }, "lines") => self.lines_node(Section::Lines, json!({ "tools_label_lines": { "of": r, "rule": rule } }))?,
            (Lazy::LabelRule { r, rule }, "area") => self.area(json!({ "tools_label_area": { "of": r, "rule": rule } }))?,
            (Lazy::LabelRule { r, rule }, "dots") => self.area(json!({ "tools_label_area": { "of": r, "rule": rule, "dots": true } }))?,
            (lazy, part) => {
                let what = match lazy {
                    Lazy::Graticule(_) => "a graticule has lines, keys, labels and labelArea",
                    Lazy::Labels { .. } => "labels have lines, area, dots and a part per rule id",
                    Lazy::LabelRule { .. } => "a label rule has lines, area and dots",
                };
                return self.fail(format!("no part {part}: {what}"));
            }
        };
        self.top().memo.insert(key, made.clone());
        Ok(made)
    }

    /// The parts a lazy module part has, for a binding to expose.
    pub fn parts(of: &Lazy) -> Vec<String> {
        match of {
            Lazy::Graticule(_) => ["lines", "keys", "labels", "labelArea"].map(String::from).to_vec(),
            Lazy::Labels { rules, .. } => ["lines", "area", "dots"].iter().map(|s| s.to_string()).chain(rules.iter().cloned()).collect(),
            Lazy::LabelRule { .. } => ["lines", "area", "dots"].map(String::from).to_vec(),
        }
    }

    // -----------------------------------------------------------------------
    // features
    // -----------------------------------------------------------------------

    fn features_json(&mut self, args: &[Arg]) -> Result<Arg> {
        let url = match at(args, 0) {
            Some(Arg::Str(s)) if !s.trim().is_empty() => s.trim().to_string(),
            other => return self.fail(format!("features.json takes a URL or a file path, got {}", describe(other))),
        };
        let paths = self.object(at(args, 1), "features.json paths")?.clone();
        for k in ["lat", "lng"] {
            if !matches!(paths.get(k), Some(Arg::Str(s)) if !s.is_empty()) {
                let what = if k == "lat" { "latitude" } else { "longitude" };
                return self.fail(format!("features.json needs {k}, the dot-path to each item's {what}"));
            }
        }
        if !matches!(paths.get("items"), None | Some(Arg::Null) | Some(Arg::Str(_))) {
            return self.fail("features.json items is a dot-path, like \"data\"");
        }
        if let Some(extra) = paths.without(&["items", "lat", "lng"]).keys().next() {
            return self.fail(format!("features.json: unknown key {extra} (items, lat, lng)"));
        }
        let items = paths.get("items").and_then(Arg::as_str).unwrap_or("").to_string();
        self.new_features(json!({ "data_select": { "json": {
            "url": url, "items": items, "lat": paths.get("lat").map(Arg::to_json), "lng": paths.get("lng").map(Arg::to_json)
        } } }))
    }

    fn features_contours(&mut self, args: &[Arg]) -> Result<Arg> {
        let o = self.object(at(args, 0), "features.contours")?.clone();
        let allowed = ["step", "from", "to", "index", "blur", "minLength"];
        if let Some(extra) = o.without(&allowed).keys().next() {
            return self.fail(format!("features.contours: unknown key {extra} ({})", allowed.join(", ")));
        }
        for k in ["step", "from", "to", "index", "blur"] {
            if let Some(v) = o.get(k) {
                if v.as_finite().is_none() {
                    return self.fail(format!("features.contours {k} is a number, got {}", describe(Some(v))));
                }
            }
        }
        let Some(step) = o.get("step") else {
            return self.fail("features.contours needs a step, the metres between contours");
        };
        let mut spec = Map::new();
        spec.insert("step".into(), step.to_json());
        for k in ["from", "to", "index", "blur"] {
            if let Some(v) = o.get(k) {
                spec.insert(k.into(), v.to_json());
            }
        }
        if o.has("minLength") {
            spec.insert("min_length".into(), crate::arg::number(self.mm(o.get("minLength"), "features.contours minLength")?));
        }
        self.new_features(json!({ "terrain_select": { "contours": Value::Object(spec) } }))
    }

    fn features_soundings(&mut self, args: &[Arg]) -> Result<Arg> {
        let o = self.object(at(args, 0), "features.soundings")?.clone();
        if let Some(extra) = o.without(&["every", "on", "blur", "jitter"]).keys().next() {
            return self.fail(format!("features.soundings: unknown key {extra} (every, on, blur, jitter)"));
        }
        if !o.has("every") {
            return self.fail("features.soundings needs every, the distance between points");
        }
        let mut spec = Map::new();
        spec.insert("every".into(), crate::arg::number(self.mm(o.get("every"), "features.soundings every")?));
        if let Some(on) = o.get("on") {
            if !matches!(on.as_str(), Some("sea" | "land" | "all")) {
                return self.fail(format!("features.soundings on is \"sea\", \"land\" or \"all\", got {}", describe(Some(on))));
            }
            spec.insert("on".into(), on.to_json());
        }
        if let Some(blur) = o.get("blur") {
            if blur.as_finite().is_none() {
                return self.fail(format!("features.soundings blur is a number, got {}", describe(Some(blur))));
            }
            spec.insert("blur".into(), blur.to_json());
        }
        if let Some(j) = o.get("jitter") {
            if !j.as_num().is_some_and(|j| (0.0..=1.0).contains(&j)) {
                return self.fail(format!("features.soundings jitter is 0 to 1, got {}", describe(Some(j))));
            }
            spec.insert("jitter".into(), j.to_json());
        }
        self.new_features(json!({ "terrain_select": { "soundings": Value::Object(spec) } }))
    }

    fn features_where(&mut self, args: &[Arg]) -> Result<Arg> {
        let r = self.need(at(args, 0), Kind::Features, "features.where")?;
        let tags = self.object(at(args, 1), "features.where tags")?.clone();
        if tags.is_empty() {
            return self.fail("features.where needs at least one tag");
        }
        for (k, v) in &tags.0 {
            if let Arg::Map(b) = v {
                let extra = b.keys().any(|x| x != "min" && x != "max");
                if extra || b.is_empty() {
                    return self.fail(format!("features.where: {k} takes {{ min, max }}, got {}", v.to_json()));
                }
                for bound in ["min", "max"] {
                    if let Some(x) = b.get(bound) {
                        if x.as_finite().is_none() {
                            return self.fail(format!("features.where: {k}.{bound} must be a number, got {}", describe(Some(x))));
                        }
                    }
                }
            }
        }
        self.new_features(json!({ "tools_where": { "of": r, "tags": Arg::Map(tags).to_json() } }))
    }

    // -----------------------------------------------------------------------
    // areas, shapes, text
    // -----------------------------------------------------------------------

    fn length_arg(&self, v: Option<&Arg>, message: &str) -> Result<Value> {
        match v {
            Some(a @ (Arg::Num(_) | Arg::Str(_))) => Ok(a.to_json()),
            other => self.fail(format!("{message}, got {}", describe(other))),
        }
    }

    fn areas_band(&mut self, args: &[Arg]) -> Result<Arg> {
        let wh = self.options(opt(args, 0), "areas.band")?;
        let options = self.options(opt(args, 1), "areas.band options")?;
        let mut w = Map::new();
        for k in ["elevMin", "elevMax", "depthMin", "depthMax"] {
            if let Some(v) = wh.get(k) {
                if v.as_finite().is_none() {
                    return self.fail(format!("areas.band {k} must be a number of metres"));
                }
                w.insert(k.into(), v.to_json());
            }
        }
        if let Some(extra) = wh.keys().find(|k| !w.contains_key(*k)) {
            return self.fail(format!("areas.band: unknown key {extra} (elevMin, elevMax, depthMin, depthMax)"));
        }
        let depth = w.contains_key("depthMin") || w.contains_key("depthMax");
        let mut op = Map::new();
        op.insert("where".into(), Value::Object(w));
        if let Some(a) = options.get("in") {
            op.insert("in_area".into(), Value::String(self.need(Some(a), Kind::Area, "areas.band in")?));
        }
        // Smoother edges than a hatching pass reads: belts, not pixels.
        let blur = match options.get("blur") {
            None | Some(Arg::Null) => json!(4),
            Some(b) => b.to_json(),
        };
        let mut style = json!({ "input_blur": blur, "mod_wobble": 0 });
        // A depth band is a fill of the sea: burin fills land unless told so.
        if depth {
            style["ground"] = json!("sea");
        }
        op.insert("style".into(), style);
        self.area(json!({ "tools_terrain_band": Value::Object(op) }))
    }

    /// A shape written in the profile: `at` is `[x, y]` on the page (lengths
    /// or "50%") or `{ lat, lng }` on the ground; the outline is a `circle`
    /// radius, a `rect: [w, h]`, a regular polygon `{ sides, r }`, or
    /// `points: [[dx, dy], …]` in mm from `at`. `rotate` turns it clockwise.
    fn shape_op(&self, spec: Option<&Arg>, wh: &str) -> Result<Value> {
        let spec = self.object(spec, wh)?;
        let at = match spec.get("at") {
            None | Some(Arg::Null) => json!(["50%", "50%"]),
            Some(a) => a.to_json(),
        };
        let ring = |n: usize, r: f64, start: f64| -> Value {
            Value::Array(
                (0..n)
                    .map(|i| {
                        let t = start + (2.0 * PI * i as f64) / n as f64;
                        json!([crate::arg::number(r * t.cos()), crate::arg::number(r * t.sin())])
                    })
                    .collect(),
            )
        };
        let given = |k: &str| matches!(spec.get(k), Some(v) if *v != Arg::Null);
        let points = if given("circle") {
            ring(64, self.mm(spec.get("circle"), &format!("{wh}.circle"))?, 0.0)
        } else if given("sides") {
            let n = spec.get("sides").and_then(Arg::as_num).unwrap_or(0.0).max(0.0) as usize;
            ring(n, self.mm(spec.get("r"), &format!("{wh}.r"))?, -PI / 2.0)
        } else if given("rect") {
            let rect = spec.get("rect").and_then(Arg::as_list).unwrap_or(&[]);
            let half: Vec<f64> = (0..rect.len().max(2))
                .map(|i| self.mm(rect.get(i), &format!("{wh}.rect[{i}]")).map(|v| v / 2.0))
                .collect::<Result<_>>()?;
            let (w, h) = (half[0], half[1]);
            json!([[-w, -h], [w, -h], [w, h], [-w, h]].map(|p| p.map(crate::arg::number)))
        } else if let Some(Arg::List(p)) = spec.get("points") {
            Arg::List(p.clone()).to_json()
        } else {
            return self.fail(format!("{wh} needs a circle, rect, sides + r, or points"));
        };
        let rotate = match spec.get("rotate") {
            None | Some(Arg::Null) => json!(0),
            Some(r) => r.to_json(),
        };
        Ok(json!({ "at": at, "points": points, "rotate": rotate }))
    }

    /// A line of text centred on `at` (as for shapes); `size` is the capital
    /// height; `font` as in `modules.labels`.
    fn text_op(&self, spec: Option<&Arg>, wh: &str) -> Result<Value> {
        let spec = self.object(spec, wh)?;
        let text = match spec.get("text") {
            Some(Arg::Str(s)) if !s.is_empty() => s.clone(),
            _ => return self.fail(format!("{wh}.text is a string")),
        };
        let or = |k: &str, d: Value| match spec.get(k) {
            None | Some(Arg::Null) => d,
            Some(v) => v.to_json(),
        };
        let mut op = json!({ "at": or("at", json!(["50%", "50%"])), "text": text, "size": or("size", json!("4mm")), "rotate": or("rotate", json!(0)) });
        if let Some(f) = spec.get("font").filter(|f| **f != Arg::Null) {
            op["font"] = self.label_font(f, &format!("{wh}.font"))?;
        }
        if let Some(c) = spec.get("counters").filter(|c| **c != Arg::Null) {
            match c.as_str() {
                Some("open") => {
                    op["openCounters"] = json!(true);
                }
                Some("closed") => {}
                _ => return self.fail(format!("{wh}.counters is \"open\" or \"closed\", got {}", describe(Some(c)))),
            }
        }
        Ok(op)
    }

    /// A draw.land Hershey id (`"futural"`) or `{ origin: "drawland" |
    /// "stroke" | "google", … }`.
    fn label_font(&self, font: &Arg, wh: &str) -> Result<Value> {
        if let Arg::Str(s) = font {
            if s.is_empty() {
                return self.fail(format!("{wh}: font id is a non-empty string"));
            }
            return Ok(json!(s));
        }
        let o = self.object(Some(font), wh)?;
        let nonempty = |k: &str| matches!(o.get(k), Some(Arg::Str(s)) if !s.is_empty());
        match o.get("origin").and_then(Arg::as_str) {
            Some("drawland") => {
                if !nonempty("id") {
                    return self.fail(format!("{wh}: drawland font needs id"));
                }
                let mut out = json!({ "origin": "drawland", "id": o.get("id").map(Arg::to_json) });
                if let Some(url) = o.get("url").filter(|u| **u != Arg::Null) {
                    if !nonempty("url") {
                        return self.fail(format!("{wh}: drawland font url is a non-empty string"));
                    }
                    out["url"] = url.to_json();
                }
                Ok(out)
            }
            Some("stroke") => {
                if !nonempty("url") {
                    return self.fail(format!("{wh}: stroke font needs url"));
                }
                Ok(json!({ "origin": "stroke", "url": o.get("url").map(Arg::to_json) }))
            }
            Some("google") => {
                if !nonempty("family") {
                    return self.fail(format!("{wh}: google font needs family"));
                }
                let weight = match o.get("weight") {
                    None => 400.0,
                    Some(w) => match w.as_finite() {
                        Some(w) => w,
                        None => return self.fail(format!("{wh}: google font weight is a number, got {}", describe(Some(w)))),
                    },
                };
                let mut out = json!({ "origin": "google", "family": o.get("family").map(Arg::to_json), "weight": crate::arg::number(weight.round()) });
                if o.get("italic").is_some_and(Arg::truthy) {
                    out["italic"] = json!(true);
                }
                Ok(out)
            }
            _ => self.fail(format!("{wh}: font origin is \"drawland\", \"stroke\", or \"google\"")),
        }
    }

    fn page_point(&self, p: Option<&Arg>, wh: &str) -> Result<(f64, f64)> {
        match p {
            Some(Arg::List(l)) if l.len() == 2 => Ok((self.mm(l.first(), &format!("{wh}[0]"))?, self.mm(l.get(1), &format!("{wh}[1]"))?)),
            other => self.fail(format!("{wh} takes [x, y] in page millimetres, got {}", describe(other))),
        }
    }

    fn ground_point(&self, p: Option<&Arg>, wh: &str) -> Result<(f64, f64)> {
        let o = p.and_then(Arg::as_map);
        match o.map(|o| (o.get("lat").and_then(Arg::as_finite), o.get("lng").and_then(Arg::as_finite))) {
            Some((Some(lat), Some(lng))) => Ok((lat, lng)),
            _ => self.fail(format!("{wh} takes {{ lat, lng }} in degrees, got {}", describe(p))),
        }
    }

    // -----------------------------------------------------------------------
    // lines.terrain, burin
    // -----------------------------------------------------------------------

    fn lines_terrain(&mut self, o: Obj) -> Result<Arg> {
        let mut op = Map::new();
        if let Some(a) = o.get("in") {
            op.insert("in_area".into(), Value::String(self.need(Some(a), Kind::Area, "lines.terrain in")?));
        }
        for k in ["color", "width", "angle", "layer"] {
            if let Some(v) = o.get(k) {
                op.insert(k.into(), v.to_json());
            }
        }
        if let Some(s) = o.get("shore") {
            op.insert("shore".into(), Value::Bool(s.truthy()));
        }
        if let Some(d) = o.get("draw") {
            match d.as_str() {
                Some("lines" | "fill" | "dots") => {
                    op.insert("draw".into(), d.to_json());
                }
                _ => return self.fail(format!("lines.terrain draw must be \"lines\", \"fill\" or \"dots\", not {}", d.to_json())),
            }
        }
        if let Some(wh) = o.get("where") {
            let wh = self.object(Some(wh), "lines.terrain where")?;
            let mut w = Map::new();
            for k in ["elevMin", "elevMax", "depthMin", "depthMax"] {
                if let Some(v) = wh.get(k) {
                    if v.as_finite().is_none() {
                        return self.fail(format!("lines.terrain where.{k} must be a number of metres"));
                    }
                    w.insert(k.into(), v.to_json());
                }
            }
            if !w.is_empty() {
                op.insert("where".into(), Value::Object(w));
            }
        }
        // `style` and any other key are burin style parameters, by name.
        let mut params = match o.get("style") {
            Some(Arg::Map(s)) => s.clone(),
            _ => Obj::new(),
        };
        for (k, v) in o.without(&["in", "color", "width", "angle", "layer", "shore", "style", "where", "draw"]).0 {
            params.set(&k, v);
        }
        if !params.is_empty() {
            op.insert("style".into(), Arg::Map(params).to_json());
        }
        self.lines_node(Section::Lines, json!({ "tools_terrain": Value::Object(op) }))
    }

    /// `burin.terrain(atlas, options)`: the atlas is the burin document as
    /// burin builds it (from JavaScript, a function run by burin's own
    /// builder; elsewhere, the document's JSON).
    fn burin_terrain(&mut self, args: &[Arg]) -> Result<Arg> {
        let atlas = match at(args, 0) {
            Some(a @ Arg::Map(_)) => a.to_json(),
            Some(Arg::Str(s)) => match serde_json::from_str::<Value>(s) {
                Ok(v) => v,
                Err(e) => return self.fail(format!("burin.terrain: the burin document is not JSON: {e}")),
            },
            other => return self.fail(format!("burin.terrain expects a burin document, got {}", describe(other))),
        };
        let options = self.options(opt(args, 1), "burin.terrain options")?;
        let mut op = Map::new();
        op.insert("atlas".into(), Value::String(atlas.to_string()));
        if let Some(a) = options.get("in") {
            op.insert("in_area".into(), Value::String(self.need(Some(a), Kind::Area, "burin.terrain in")?));
        }
        if let Some(regions) = options.get("regions") {
            let regions = self.object(Some(regions), "burin.terrain regions")?.clone();
            let mut named = Map::new();
            for (name, reg) in &regions.0 {
                named.insert(name.clone(), Value::String(self.need(Some(reg), Kind::Area, &format!("burin.terrain regions.{name}"))?));
            }
            op.insert("regions".into(), Value::Object(named));
        }
        if let Some(w) = options.get("water") {
            op.insert("water".into(), Value::String(self.need(Some(w), Kind::Area, "burin.terrain water")?));
        }
        self.lines_node(Section::Lines, json!({ "tools_burin": Value::Object(op) }))
    }

    // -----------------------------------------------------------------------
    // ink
    // -----------------------------------------------------------------------

    fn unit(&self, t: Option<&Arg>, wh: &str) -> Result<f64> {
        match t.and_then(Arg::as_num).filter(|n| !n.is_nan()) {
            Some(t) => Ok(t.clamp(0.0, 1.0)),
            None => self.fail(format!("{wh}: t is a number from 0 to 1, got {}", describe(t))),
        }
    }

    fn rgb(&self, hex: &Arg, wh: &str) -> Result<[f64; 3]> {
        let parsed = hex.as_str().and_then(|s| {
            let h = s.strip_prefix('#')?;
            if !h.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let h: String = match h.len() {
                3 => h.chars().flat_map(|c| [c, c]).collect(),
                6 => h.to_string(),
                _ => return None,
            };
            let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(f64::from);
            Some([c(0)?, c(2)?, c(4)?])
        });
        match parsed {
            Some(c) => Ok(c),
            None => self.fail(format!("{wh}: colours are \"#rgb\" or \"#rrggbb\", got {}", describe(Some(hex)))),
        }
    }

    fn ink_ramp(&self, args: &[Arg]) -> Result<Arg> {
        let stops = match at(args, 0) {
            Some(Arg::List(l)) if l.len() >= 2 => l,
            other => return self.fail(format!("ink.ramp needs two colours or more, got {}", describe(other))),
        };
        let cs: Vec<[f64; 3]> = stops.iter().map(|s| self.rgb(s, "ink.ramp")).collect::<Result<_>>()?;
        let pos = self.unit(at(args, 1), "ink.ramp")? * (cs.len() - 1) as f64;
        let i = (pos.floor() as usize).min(cs.len() - 2);
        let f = pos - i as f64;
        let mix: String = (0..3).map(|k| format!("{:02x}", (cs[i][k] + (cs[i + 1][k] - cs[i][k]) * f).round() as u8)).collect();
        Ok(Arg::Str(format!("#{mix}")))
    }

    // -----------------------------------------------------------------------
    // modules
    // -----------------------------------------------------------------------

    fn road_measure(&self, v: &Arg, wh: &str) -> Result<Value> {
        if let Some(n) = v.as_finite() {
            return Ok(crate::arg::number(n));
        }
        if let Arg::Map(o) = v {
            if o.len() == 1 {
                let (k, x) = &o.0[0];
                if (k == "ground" || k == "paper") && x.as_finite().is_some() {
                    return Ok(json!({ k.clone(): x.to_json() }));
                }
            }
        }
        self.fail(format!("{wh} expects a number or {{ ground: number }} / {{ paper: number }}, got {}", describe(Some(v))))
    }

    fn modules_roads(&mut self, args: &[Arg]) -> Result<Arg> {
        const KEYS: [(&str, &str); 9] = [
            ("widthMode", "width_mode"),
            ("widthMultiplier", "width_multiplier"),
            ("useWidthTag", "use_width_tag"),
            ("multiplyByLanes", "multiply_by_lanes"),
            ("inferLanesByClass", "infer_lanes_by_class"),
            ("centerlineMinWidth", "centerline_min_width"),
            ("jointClose", "joint_close"),
            ("clearance", "clearance"),
            ("classes", "classes"),
        ];
        const CLASS_KEYS: [(&str, &str); 7] = [
            ("ground", "ground"),
            ("paper", "paper"),
            ("width", "width"),
            ("lanes", "lanes"),
            ("centerline", "centerline"),
            ("singleLine", "single_line"),
            ("clearance", "clearance"),
        ];
        let options = self.options(opt(args, 0), "modules.roads options")?;
        let graph_key = |k: &str| KEYS.iter().find(|(js, _)| *js == k).map(|(_, g)| *g);
        let mut op = Map::new();
        for (k, v) in &options.0 {
            match k.as_str() {
                "from" => {
                    op.insert("from".into(), Value::String(self.need(Some(v), Kind::Features, "modules.roads from")?));
                }
                "widthMode" => {
                    if !matches!(v.as_str(), Some("ground" | "paper")) {
                        return self.fail(format!("modules.roads widthMode expects \"ground\" or \"paper\", got {}", describe(Some(v))));
                    }
                    op.insert("width_mode".into(), v.to_json());
                }
                "widthMultiplier" => {
                    if !v.as_finite().is_some_and(|n| n > 0.0) {
                        return self.fail(format!("modules.roads widthMultiplier expects a positive number, got {}", describe(Some(v))));
                    }
                    op.insert("width_multiplier".into(), v.to_json());
                }
                "useWidthTag" | "multiplyByLanes" | "inferLanesByClass" => {
                    op.insert(graph_key(k).expect("listed").into(), Value::Bool(v.truthy()));
                }
                "centerlineMinWidth" | "jointClose" | "clearance" => {
                    op.insert(graph_key(k).expect("listed").into(), self.road_measure(v, &format!("modules.roads {k}"))?);
                }
                "classes" => {
                    let classes = self.object(Some(v), "modules.roads classes")?.clone();
                    let mut out = Map::new();
                    for (cls, spec) in &classes.0 {
                        let cwhere = format!("modules.roads classes.{cls}");
                        if let Some(n) = spec.as_finite() {
                            out.insert(cls.clone(), crate::arg::number(n));
                            continue;
                        }
                        let spec = self.object(Some(spec), &cwhere)?;
                        let mut entry = Map::new();
                        for (ck, cv) in &spec.0 {
                            let Some((_, gk)) = CLASS_KEYS.iter().find(|(js, _)| js == ck) else {
                                let allowed: Vec<&str> = CLASS_KEYS.iter().map(|(js, _)| *js).collect();
                                return self.fail(format!("{cwhere}: unknown key {ck} ({})", allowed.join(", ")));
                            };
                            match ck.as_str() {
                                "ground" | "paper" | "width" => {
                                    if cv.as_finite().is_none() {
                                        return self.fail(format!("{cwhere}.{ck} expects a number, got {}", describe(Some(cv))));
                                    }
                                    entry.insert(gk.to_string(), cv.to_json());
                                }
                                "lanes" => {
                                    if !cv.as_finite().is_some_and(|n| n > 0.0) {
                                        return self.fail(format!("{cwhere}.lanes expects a positive number, got {}", describe(Some(cv))));
                                    }
                                    entry.insert("lanes".into(), cv.to_json());
                                }
                                "centerline" | "singleLine" => {
                                    entry.insert(gk.to_string(), Value::Bool(cv.truthy()));
                                }
                                _ => {
                                    entry.insert("clearance".into(), self.road_measure(cv, &format!("{cwhere}.clearance"))?);
                                }
                            }
                        }
                        out.insert(cls.clone(), Value::Object(entry));
                    }
                    op.insert("classes".into(), Value::Object(out));
                }
                other => {
                    let allowed: Vec<&str> = std::iter::once("from").chain(KEYS.iter().map(|(js, _)| *js)).collect();
                    return self.fail(format!("modules.roads: unknown option {other} ({})", allowed.join(", ")));
                }
            }
        }
        let r = self.make(Section::Areas, json!({ "module_roads": Value::Object(op) }))?;
        let out = |name: &str| Arg::Lines { r: format!("roads.{name}"), module: Some("roads".into()) };
        Ok(Arg::Map(Obj(vec![
            ("area".into(), Arg::Area { r, mask: "roads.area".into() }),
            ("outlines".into(), out("outlines")),
            ("details".into(), out("details")),
            ("trails".into(), out("trails")),
            ("centerlines".into(), out("centerlines")),
            ("seams".into(), out("seams")),
            ("rails".into(), out("rails")),
        ])))
    }

    /// `paint({ id, from, fill | line | text, halo })`: a layer of the
    /// screen. What `from` is decides the look it takes: an area a `fill`,
    /// lines a `line`, features a `text`.
    fn paint(&mut self, args: &[Arg]) -> Result<Arg> {
        use drawland_core::paint::{Halo, Look};
        let spec = self.object(at(args, 0), "paint")?.clone();
        if let Some(extra) = spec.without(&["id", "from", "fill", "line", "text", "halo"]).keys().next() {
            return self.fail(format!("paint: unknown key {extra} (id, from, fill, line, text, halo)"));
        }
        let id = match spec.get("id") {
            Some(Arg::Str(s)) if !s.trim().is_empty() => s.clone(),
            other => return self.fail(format!("paint: id names the layer, got {}", describe(other))),
        };
        let (from, key) = match spec.get("from") {
            Some(Arg::Area { r, .. }) => (r.clone(), "fill"),
            Some(Arg::Lines { r, .. }) => (r.clone(), "line"),
            Some(Arg::Features(r)) => (r.clone(), "text"),
            other => {
                return self.fail(format!(
                    "paint: from expects an area (filled), lines (lined) or features (named), got {}",
                    describe(other)
                ))
            }
        };
        if let Some(other) = ["fill", "line", "text"].into_iter().find(|k| *k != key && spec.has(k)) {
            let what = match key {
                "fill" => "an area",
                "line" => "lines",
                _ => "features",
            };
            return self.fail(format!("paint {id:?}: from is {what}, painted with {key}, not {other}"));
        }
        let body = spec.get(key).map(Arg::to_json).unwrap_or_else(|| Value::Object(Map::new()));
        let parse = |e: serde_json::Error| format!("paint {id:?}: {key}: {e}");
        let look = match key {
            "fill" => Look::Fill(serde_json::from_value(body).map_err(parse).or_else(|e| self.fail(e))?),
            "line" => Look::Line(serde_json::from_value(body).map_err(parse).or_else(|e| self.fail(e))?),
            _ => Look::Text(serde_json::from_value(body).map_err(parse).or_else(|e| self.fail(e))?),
        };
        let halo: Halo = match spec.get("halo") {
            None | Some(Arg::Null) => Halo::default(),
            Some(h) => serde_json::from_value(h.to_json())
                .or_else(|_| self.fail(format!("paint {id:?}: halo is pixels or cells, n or [across, down], got {}", describe(Some(h)))))?,
        };
        Ok(Arg::Str(self.add_paint(id, &from, look, halo)?))
    }

    fn modules_graticule(&mut self, args: &[Arg]) -> Result<Arg> {
        let o = self.options(opt(args, 0), "modules.graticule options")?;
        if let Some(extra) = o.without(&["in", "step", "ticks", "band", "gap", "size", "font"]).keys().next() {
            return self.fail(format!("modules.graticule: unknown option {extra} (in, step, ticks, band, gap, size, font)"));
        }
        let within = self.need(o.get("in"), Kind::Area, "modules.graticule in")?;
        let step = o.get("step");
        if !step.and_then(Arg::as_num).is_some_and(|s| s > 0.0) {
            return self.fail(format!("modules.graticule: step is minutes of arc between lines, got {}", describe(step)));
        }
        if let Some(t) = o.get("ticks") {
            if !t.as_num().is_some_and(|t| t >= 0.0) {
                return self.fail(format!("modules.graticule: ticks is minutes of arc per border key, got {}", describe(Some(t))));
            }
        }
        let mut look = Map::new();
        look.insert("in".into(), Value::String(within));
        look.insert("step".into(), step.map(Arg::to_json).unwrap_or(Value::Null));
        for k in ["ticks", "band", "gap", "size"] {
            if let Some(v) = o.get(k) {
                look.insert(k.into(), v.to_json());
            }
        }
        if let Some(f) = o.get("font") {
            look.insert("font".into(), self.label_font(f, "modules.graticule font")?);
        }
        Ok(Arg::Lazy(Lazy::Graticule(Value::Object(look))))
    }

    fn graticule_part(&mut self, look: &Value, section: Section, part: &str) -> Result<Arg> {
        let mut op = look.clone();
        op["part"] = json!(part);
        match section {
            Section::Lines => self.lines_node(Section::Lines, json!({ "tools_graticule": op })),
            _ => self.area(json!({ "tools_graticule_area": op })),
        }
    }

    fn label_look(&self, look: &Obj, wh: &str, what: &str, others: &[&str]) -> Result<Map<String, Value>> {
        const LOOK: [(&str, &str); 11] = [
            ("size", "size"),
            ("minSize", "min_size"),
            ("padding", "padding"),
            ("spacing", "spacing"),
            ("curved", "curved"),
            ("upper", "upper"),
            ("font", "font"),
            ("buffer", "buffer"),
            ("bufferStep", "buffer_step"),
            ("keepCenterline", "keep_centerline"),
            ("marker", "marker"),
        ];
        let mut out = Map::new();
        for (k, v) in &look.0 {
            let Some((_, g)) = LOOK.iter().find(|(js, _)| js == k) else {
                let all: Vec<&str> = others.iter().copied().chain(LOOK.iter().map(|(js, _)| *js)).collect();
                return self.fail(format!("{wh}: unknown {what} {k} ({})", all.join(", ")));
            };
            match k.as_str() {
                "font" => {
                    out.insert("font".into(), self.label_font(v, &format!("{wh} font"))?);
                }
                "curved" | "upper" | "keepCenterline" => {
                    out.insert(g.to_string(), Value::Bool(v.truthy()));
                }
                "buffer" => {
                    if !v.as_num().is_some_and(|n| n.fract() == 0.0 && n >= 0.0) {
                        return self.fail(format!("{wh}: buffer is a non-negative integer, got {}", describe(Some(v))));
                    }
                    out.insert("buffer".into(), v.to_json());
                }
                _ => {
                    out.insert(g.to_string(), v.to_json());
                }
            }
        }
        Ok(out)
    }

    fn modules_labels(&mut self, args: &[Arg]) -> Result<Arg> {
        let o = self.options(opt(args, 0), "modules.labels options")?;
        let sets: Vec<Arg> = match o.get("from") {
            Some(Arg::List(l)) => l.clone(),
            Some(f) => vec![f.clone()],
            None => vec![],
        };
        if sets.is_empty() && matches!(o.get("from"), Some(Arg::List(_))) {
            return self.fail("modules.labels: from needs at least one feature set");
        }
        let sets: &[Option<&Arg>] = &if sets.is_empty() { vec![None] } else { sets.iter().map(Some).collect() };
        let n = sets.len();
        let from = sets
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let wh = if n > 1 { format!("modules.labels from (input {})", i + 1) } else { "modules.labels from".into() };
                self.need(*f, Kind::Features, &wh).map(Value::String)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut op = Map::new();
        op.insert("from".into(), Value::Array(from));
        let look = o.without(&["from", "in", "avoid", "keys", "rules"]);
        op.extend(self.label_look(&look, "modules.labels", "option", &["from", "in", "avoid", "keys", "rules"])?);
        if let Some(a) = o.get("in") {
            op.insert("in".into(), Value::String(self.need(Some(a), Kind::Area, "modules.labels in")?));
        }
        if let Some(a) = o.get("avoid") {
            op.insert("avoid".into(), Value::String(self.need(Some(a), Kind::Area, "modules.labels avoid")?));
        }
        if let Some(keys) = o.get("keys") {
            let ks: Vec<Arg> = match keys {
                Arg::List(l) => l.clone(),
                k => vec![k.clone()],
            };
            if ks.is_empty() || !ks.iter().all(|k| matches!(k, Arg::Str(_))) {
                return self.fail(format!("modules.labels: keys is a tag or a list of tags, got {}", describe(Some(keys))));
            }
            op.insert("keys".into(), Arg::List(ks).to_json());
        }
        let mut ids = Vec::new();
        if let Some(rules) = o.get("rules") {
            let rules = match rules {
                Arg::List(l) if !l.is_empty() => l.clone(),
                other => return self.fail(format!("modules.labels: rules is a non-empty list, got {}", describe(Some(other)))),
            };
            let mut out = Vec::new();
            for (i, rule) in rules.iter().enumerate() {
                let wh = format!("modules.labels rules[{i}]");
                let rule = self.object(Some(rule), &wh)?;
                let tags = self.object(rule.get("tags"), &format!("{wh}.tags"))?;
                if tags.is_empty() {
                    return self.fail(format!("{wh}.tags needs at least one tag"));
                }
                let mut entry = Map::new();
                entry.insert("tags".into(), Arg::Map(tags.clone()).to_json());
                entry.extend(self.label_look(&rule.without(&["tags", "id"]), &wh, "key", &["tags", "id"])?);
                if let Some(id) = rule.get("id") {
                    let ok = id.as_str().is_some_and(|s| {
                        let mut c = s.chars();
                        c.next().is_some_and(|f| f == '_' || f.is_ascii_alphabetic()) && c.all(|x| x == '_' || x.is_ascii_alphanumeric())
                    });
                    if !ok {
                        return self.fail(format!("{wh}: id is a name (letters, digits, underscore), got {}", describe(Some(id))));
                    }
                    let id = id.as_str().expect("checked").to_string();
                    if ids.contains(&id) {
                        return self.fail(format!("modules.labels: duplicate rule id {id}"));
                    }
                    entry.insert("id".into(), json!(id));
                    ids.push(id);
                }
                out.push(Value::Object(entry));
            }
            op.insert("rules".into(), Value::Array(out));
        }
        let r = self.make(Section::Lines, json!({ "tools_labels": Value::Object(op) }))?;
        Ok(Arg::Lazy(Lazy::Labels { r, rules: ids }))
    }

    // -----------------------------------------------------------------------
    // compose: the plotter steps
    // -----------------------------------------------------------------------

    fn optimize(&mut self, args: &[Arg], name: &str, shorthand: Option<&str>, keys: &[(&str, &str)], is_shorthand: fn(&Arg) -> bool) -> Result<Arg> {
        let wh = format!("compose.{name}");
        let (a, b) = (at(args, 0).filter(|a| **a != Arg::Null), at(args, 1));
        let mut layers: Vec<String> = Vec::new();
        let raw: Option<Arg> = match a {
            None => b.cloned(),
            Some(Arg::List(l)) => {
                layers = l.iter().map(stringify).collect();
                b.cloned()
            }
            Some(o @ Arg::Map(_)) => {
                if b.is_some() {
                    return self.fail(format!("{wh}: unexpected second argument when first is options"));
                }
                Some(o.clone())
            }
            Some(v @ (Arg::Str(_) | Arg::Num(_))) => {
                if b.is_some() {
                    if !matches!(v, Arg::Str(_)) {
                        return self.fail(format!("{wh}: layers expects a draw id or list of draw ids, got {}", describe(a)));
                    }
                    layers = vec![stringify(v)];
                    b.cloned()
                } else if shorthand.is_some() && is_shorthand(v) {
                    Some(Arg::Map(Obj(vec![(shorthand.unwrap().to_string(), v.clone())])))
                } else if let Arg::Str(s) = v {
                    layers = vec![s.clone()];
                    None
                } else {
                    return self.fail(format!("{wh}: unexpected {}", describe(a)));
                }
            }
            other => return self.fail(format!("{wh}: unexpected {}", describe(other))),
        };
        let mut spec = Map::new();
        let mut except: Vec<String> = Vec::new();
        if let Some(raw) = raw {
            match (shorthand, &raw) {
                (Some(k), v) if is_shorthand(v) => {
                    let g = keys.iter().find(|(js, _)| *js == k).map(|(_, g)| *g).unwrap_or(k);
                    spec.insert(g.into(), v.to_json());
                }
                _ => {
                    let o = self.object(Some(&raw), &wh)?;
                    for (k, v) in &o.0 {
                        if k == "except" {
                            match v {
                                Arg::List(l) => except = l.iter().map(stringify).collect(),
                                other => return self.fail(format!("{wh}: except is a list of draw ids, got {}", describe(Some(other)))),
                            }
                        } else if let Some((_, g)) = keys.iter().find(|(js, _)| js == k) {
                            spec.insert(g.to_string(), v.to_json());
                        } else {
                            let allowed: Vec<&str> = keys.iter().map(|(js, _)| *js).chain(std::iter::once("except")).collect();
                            return self.fail(format!("{wh}: unknown key {k} ({})", allowed.join(", ")));
                        }
                    }
                }
            }
        }
        let mut step = Map::new();
        step.insert(name.into(), Value::Object(spec));
        if !layers.is_empty() {
            step.insert("layers".into(), json!(layers));
        }
        if !except.is_empty() {
            step.insert("except".into(), json!(except));
        }
        self.add_optimize(Value::Object(step))?;
        Ok(Arg::Null)
    }

    // -----------------------------------------------------------------------
    // output and its warps
    // -----------------------------------------------------------------------

    /// The output, warps and all, except a `remap`: a function, which the
    /// caller samples with [`Graph::remap_grid`] and [`Graph::remap_mesh`].
    fn set_plain_output(&mut self, spec: &Obj) -> Result<()> {
        let warps: Vec<Arg> = match spec.get("warp") {
            None | Some(Arg::Null) => vec![],
            Some(Arg::List(l)) => l.clone(),
            Some(w) => vec![w.clone()],
        };
        if warps.iter().any(is_remap) {
            return self.fail("output: a remap warp is a function: call output from a script, or sample it with Graph::remap_grid");
        }
        self.set_output(spec)
    }

    /// `output` with its `remap` warps still to sample: the output set with
    /// none, and whether to sample them (not in a draft, which leaves them
    /// out and keeps the others).
    pub fn output_begin(&mut self, spec: &Obj) -> Result<bool> {
        let spec = self.object(Some(&Arg::Map(spec.clone())), "output")?.clone();
        let warps: Vec<Arg> = match spec.get("warp") {
            None | Some(Arg::Null) => vec![],
            Some(Arg::List(l)) => l.clone(),
            Some(w) => vec![w.clone()],
        };
        if !warps.iter().any(is_remap) {
            self.set_output(&spec)?;
            return Ok(false);
        }
        let mut plain = spec.clone();
        if self.ask_draft() {
            plain.set("warp", Arg::List(warps.into_iter().filter(|w| !is_remap(w)).collect()));
            self.set_output(&plain)?;
            return Ok(false);
        }
        plain.set("warp", Arg::List(vec![]));
        self.set_output(&plain)?;
        Ok(true)
    }

    /// The warps done so far set on the output, so `page.toWorld` answers
    /// through them while the next `remap` is sampled.
    pub fn output_warps(&mut self, done: &[Arg]) -> Result<()> {
        self.set_warp(&Arg::List(done.to_vec()))
    }

    /// The page points the `remap` warp at `n` (its `{ remap, grid }`) is
    /// sampled at: `(grid + 1)²` of them, row by row.
    pub fn remap_grid(&self, warp: &Obj, n: usize) -> Result<(usize, usize, Vec<[f64; 2]>)> {
        if !matches!(warp.get("remap"), Some(Arg::Func)) {
            return self.fail(format!("output.warp[{n}].remap is a function ([x, y]) => [x, y], got {}", describe(warp.get("remap"))));
        }
        if let Some(k) = warp.keys().find(|k| *k != "remap" && *k != "grid") {
            return self.fail(format!("output.warp[{n}]: unknown key {k}; a remap takes remap and grid"));
        }
        let grid = warp.get("grid").cloned().unwrap_or(Arg::Num(48.0));
        let (gx, gy) = match &grid {
            Arg::List(l) if l.len() == 2 => (l[0].clone(), l[1].clone()),
            g => (g.clone(), g.clone()),
        };
        let cells = |g: &Arg| g.as_num().filter(|n| n.fract() == 0.0 && (1.0..=256.0).contains(n)).map(|n| n as usize);
        let (Some(gx), Some(gy)) = (cells(&gx), cells(&gy)) else {
            return self.fail(format!(
                "output.warp[{n}].grid is a whole number of cells from 1 to 256 (or [across, down]), got {}",
                describe(Some(&grid))
            ));
        };
        let o = self.current_output("output.warp")?;
        let (width, height) = (o.width.0, o.height.0);
        let mut points = Vec::with_capacity((gx + 1) * (gy + 1));
        for j in 0..=gy {
            for i in 0..=gx {
                points.push([width * i as f64 / gx as f64, height * j as f64 / gy as f64]);
            }
        }
        Ok((gx + 1, gy + 1, points))
    }

    /// The `remap` warp at `n` as the engine draws it: a mesh of where each
    /// sampled point went. `went[i]` is what the function gave for
    /// `points[i]`.
    pub fn remap_mesh(&self, n: usize, columns: usize, rows: usize, points: &[[f64; 2]], went: &[Arg]) -> Result<Arg> {
        let mut out = Vec::with_capacity(went.len());
        for (p, to) in points.iter().zip(went) {
            match to {
                Arg::List(l) if l.len() == 2 && l.iter().all(|v| v.as_finite().is_some()) => out.push(to.clone()),
                other => {
                    return self.fail(format!(
                        "output.warp[{n}].remap([{:.2}, {:.2}]) returned {}, not [x, y] in page mm",
                        p[0],
                        p[1],
                        describe(Some(other))
                    ))
                }
            }
        }
        Ok(Arg::Map(crate::obj! { "mesh" => Arg::Map(crate::obj! {
            "columns" => columns as u32, "rows" => rows as u32, "points" => Arg::List(out)
        }) }))
    }
}

/// A warp that is a `remap`, still holding its function.
pub fn is_remap(w: &Arg) -> bool {
    matches!(w, Arg::Map(o) if o.has("remap"))
}

/// `String(v)`, as an id is written.
fn stringify(v: &Arg) -> String {
    match v {
        Arg::Str(s) => s.clone(),
        Arg::Num(n) => show_number(*n),
        Arg::Bool(b) => b.to_string(),
        Arg::Null => "null".into(),
        other => other.to_json().to_string(),
    }
}

/// `0.3mm` → `0.3`.
fn parse_mm(s: &str) -> Option<f64> {
    let s = s.trim().strip_suffix("mm")?.trim_end();
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    s.parse().ok()
}

fn is_length_value(v: &Arg) -> bool {
    match v {
        Arg::Num(_) => true,
        Arg::Str(s) => {
            let s = s.trim();
            let lower = s.to_ascii_lowercase();
            let num = ["mm", "cm", "in", "pt", "px"].iter().find_map(|u| lower.strip_suffix(u)).unwrap_or(&lower).trim_end();
            let num = num.strip_prefix('-').unwrap_or(num);
            let mut parts = num.splitn(2, '.');
            let int = parts.next().unwrap_or("");
            let frac = parts.next();
            !int.is_empty() && int.chars().all(|c| c.is_ascii_digit()) && frac.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
        }
        _ => false,
    }
}

const DIRECTIONS: [&str; 32] = [
    "lr", "left-to-right", "e", "east", "tl-br", "nw-se", "se", "southeast", "tb", "top-to-bottom", "s", "south", "tr-bl", "ne-sw", "sw",
    "southwest", "rl", "right-to-left", "w", "west", "br-tl", "se-nw", "nw", "northwest", "bt", "bottom-to-top", "n", "north", "bl-tr",
    "sw-ne", "ne", "northeast",
];

fn is_direction_value(v: &Arg) -> bool {
    match v {
        Arg::Num(n) => n.is_finite(),
        Arg::Str(s) => DIRECTIONS.contains(&s.trim().to_ascii_lowercase().as_str()),
        _ => false,
    }
}
