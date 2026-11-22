//! The library for Rust: the same functions a JavaScript or Python profile
//! calls, as methods. Handles are typed ([`Features`], [`Area`], [`Lines`]);
//! options are objects ([`crate::obj!`]), with the keys the library documents
//! (`drawland.d.ts`, the reference at draw.land/docs).
//!
//! ```
//! use drawland_graph::{obj, Graph};
//!
//! let mut g = Graph::new(Some((37.8053, -122.4436)));
//! g.output(obj! { "width" => "6in", "height" => "4in", "scale" => "1:10000" })?;
//! let water = g.features().tiles(&["water"])?;
//! let sea = g.areas().polygons(&water)?;
//! let waves = g.fills().fill(obj! { "in" => &sea, "pattern" => "waves", "spacing" => "0.8mm" })?;
//! g.draw(obj! { "id" => "water", "from" => &waves })?;
//! let centre = g.world().center()?;
//! assert!((centre.0 - 37.8053).abs() < 1e-9);
//! # Ok::<(), drawland_graph::Error>(())
//! ```

use crate::arg::{Arg, Obj};
use crate::graph::{Graph, Result};

/// A set of map features (`features.*`).
#[derive(Clone, Debug, PartialEq)]
pub struct Features(pub(crate) Arg);

/// An area (`areas.*`, `page.frame`, a module's area).
#[derive(Clone, Debug, PartialEq)]
pub struct Area(pub(crate) Arg);

/// Lines (`lines.*`, `fills.fill`, a module's output).
#[derive(Clone, Debug, PartialEq)]
pub struct Lines(pub(crate) Arg);

macro_rules! handle {
    ($t:ident) => {
        impl From<&$t> for Arg {
            fn from(h: &$t) -> Arg {
                h.0.clone()
            }
        }
        impl From<$t> for Arg {
            fn from(h: $t) -> Arg {
                h.0
            }
        }
        impl $t {
            /// The handle as the library passes it.
            pub fn arg(&self) -> &Arg {
                &self.0
            }
        }
    };
}
handle!(Features);
handle!(Area);
handle!(Lines);

impl Features {
    fn of(a: Arg) -> Features {
        Features(a)
    }
}
impl Area {
    fn of(a: Arg) -> Area {
        Area(a)
    }
}
impl Lines {
    fn of(a: Arg) -> Lines {
        Lines(a)
    }
}

/// `modules.roads(…)`: what the roads module publishes.
#[derive(Clone, Debug)]
pub struct Roads {
    pub area: Area,
    pub outlines: Lines,
    pub details: Lines,
    pub trails: Lines,
    pub centerlines: Lines,
    pub seams: Lines,
    pub rails: Lines,
}

/// `modules.labels(…)` or a graticule: parts made when asked for with
/// [`Module::area`], [`Module::part`]…
#[derive(Clone, Debug)]
pub struct Module(pub(crate) Arg);

fn list<T: Into<Arg> + Clone>(items: &[T]) -> Arg {
    Arg::List(items.iter().cloned().map(Into::into).collect())
}

fn many<T>(items: &[&T]) -> Vec<Arg>
where
    for<'a> &'a T: Into<Arg>,
{
    items.iter().map(|h| (*h).into()).collect()
}

/// `features.*`: what to read from the map.
pub struct FeaturesApi<'g>(&'g mut Graph);
/// `areas.*`: shapes on the page.
pub struct AreasApi<'g>(&'g mut Graph);
/// `lines.*`: strokes.
pub struct LinesApi<'g>(&'g mut Graph);
/// `fills.*`: patterns inside areas.
pub struct FillsApi<'g>(&'g mut Graph);
/// `modules.*`: roads, labels, graticules.
pub struct ModulesApi<'g>(&'g mut Graph);
/// `compose.*`: stacking, masks, and the plotter's steps.
pub struct ComposeApi<'g>(&'g mut Graph);
/// `page.*`: the paper, in mm from its top left.
pub struct PageApi<'g>(&'g mut Graph);
/// `world.*`: the ground under the page.
pub struct WorldApi<'g>(&'g mut Graph);

impl Graph {
    pub fn features(&mut self) -> FeaturesApi<'_> {
        FeaturesApi(self)
    }
    pub fn areas(&mut self) -> AreasApi<'_> {
        AreasApi(self)
    }
    pub fn lines(&mut self) -> LinesApi<'_> {
        LinesApi(self)
    }
    pub fn fills(&mut self) -> FillsApi<'_> {
        FillsApi(self)
    }
    pub fn modules(&mut self) -> ModulesApi<'_> {
        ModulesApi(self)
    }
    pub fn compose(&mut self) -> ComposeApi<'_> {
        ComposeApi(self)
    }
    pub fn page(&mut self) -> PageApi<'_> {
        PageApi(self)
    }
    pub fn world(&mut self) -> WorldApi<'_> {
        WorldApi(self)
    }

    /// Where the map is and on what paper: `{ lat, lng, width, height,
    /// scale, bearing, projection, padding, slices, warp, tileZoom }`. A
    /// `remap` warp is a function of page points: [`Graph::output_remap`].
    pub fn output(&mut self, spec: Obj) -> Result<()> {
        self.call("output", &[Arg::Map(spec)]).map(|_| ())
    }

    /// `output` with `remaps[i]` the function of the `i`-th `remap` warp in
    /// `spec.warp` (written `{ "remap": true, "grid": 64 }`): each runs on a
    /// grid of page points, and may read the ground under one with
    /// `g.page().to_world(p)` (through the warps before it).
    pub fn output_remap(&mut self, mut spec: Obj, remaps: &mut [&mut dyn FnMut(&mut Graph, [f64; 2]) -> Result<[f64; 2]>]) -> Result<()> {
        let warps: Vec<Arg> = match spec.get("warp") {
            None | Some(Arg::Null) => vec![],
            Some(Arg::List(l)) => l.clone(),
            Some(w) => vec![w.clone()],
        };
        let marked: Vec<Arg> = warps
            .iter()
            .map(|w| match w {
                Arg::Map(o) if o.has("remap") => {
                    let mut o = o.clone();
                    o.set("remap", Arg::Func);
                    Arg::Map(o)
                }
                other => other.clone(),
            })
            .collect();
        spec.set("warp", Arg::List(marked.clone()));
        if !self.output_begin(&spec)? {
            return Ok(());
        }
        let mut done = Vec::new();
        let mut next = 0;
        for (n, w) in marked.iter().enumerate() {
            match w {
                Arg::Map(o) if o.has("remap") => {
                    self.output_warps(&done)?;
                    let (columns, rows, points) = self.remap_grid(o, n)?;
                    let f = remaps.get_mut(next).ok_or_else(|| self.error(format!("output.warp[{n}]: no function given for this remap")))?;
                    next += 1;
                    let mut went = Vec::with_capacity(points.len());
                    for p in &points {
                        let [x, y] = f(self, *p)?;
                        went.push(Arg::List(vec![Arg::Num(x), Arg::Num(y)]));
                    }
                    done.push(self.remap_mesh(n, columns, rows, &points, &went)?);
                }
                other => done.push(other.clone()),
            }
        }
        self.output_warps(&done)
    }

    /// The map's title, for the SVG and the files.
    pub fn title(&mut self, text: &str) -> Result<()> {
        self.call("title", &[Arg::str(text)]).map(|_| ())
    }

    /// Draw lines as a group (a pen): `{ id, from, stroke }`. Its group id.
    pub fn draw(&mut self, spec: Obj) -> Result<String> {
        match self.call("draw", &[Arg::Map(spec)])? {
            Arg::Str(id) => Ok(id),
            _ => Ok(String::new()),
        }
    }

    /// Another place drawn into a rectangle of the page: `{ id, at, size,
    /// lat, lng, scale, … }`, its graph built by `style`. Its id.
    pub fn view(&mut self, spec: Obj, style: impl FnOnce(&mut Graph) -> Result<()>) -> Result<String> {
        self.view_begin(&Arg::Map(spec))?;
        let built = style(self);
        let id = self.view_end(built.is_ok())?;
        built?;
        Ok(id.as_str().unwrap_or_default().to_string())
    }
}

impl FeaturesApi<'_> {
    /// OpenFreeMap's vector tiles: presets (`"buildings"`) or rules (`"landcover:park,garden"`).
    pub fn tiles(self, items: &[&str]) -> Result<Features> {
        self.0.call("features.tiles", &[list(items)]).map(Features::of)
    }
    /// OpenStreetMap through Overpass: `{ leisure: "park" }`.
    pub fn osm(self, tags: Obj) -> Result<Features> {
        self.0.call("features.osm", &[Arg::Map(tags)]).map(Features::of)
    }
    /// A GeoJSON dataset: a URL, a file beside the profile, or inline GeoJSON.
    pub fn geojson(self, source: impl Into<Arg>) -> Result<Features> {
        self.0.call("features.geojson", &[source.into()]).map(Features::of)
    }
    /// Points from plain JSON: `{ items, lat, lng }` as dot-paths.
    pub fn json(self, url: &str, paths: Obj) -> Result<Features> {
        self.0.call("features.json", &[Arg::str(url), Arg::Map(paths)]).map(Features::of)
    }
    /// Contour lines from the elevation: `{ step, from, to, index, blur, minLength }`.
    pub fn contours(self, options: Obj) -> Result<Features> {
        self.0.call("features.contours", &[Arg::Map(options)]).map(Features::of)
    }
    /// Depth soundings: `{ every, on, blur, jitter }`.
    pub fn soundings(self, options: Obj) -> Result<Features> {
        self.0.call("features.soundings", &[Arg::Map(options)]).map(Features::of)
    }
    /// The features whose tags pass every test.
    pub fn filter(self, of: &Features, tags: Obj) -> Result<Features> {
        self.0.call("features.where", &[of.into(), Arg::Map(tags)]).map(Features::of)
    }
}

impl AreasApi<'_> {
    pub fn polygons(self, of: &Features) -> Result<Area> {
        self.0.call("areas.polygons", &[of.into()]).map(Area::of)
    }
    pub fn union(self, areas: &[&Area]) -> Result<Area> {
        self.0.call("areas.union", &many(areas)).map(Area::of)
    }
    pub fn difference(self, base: &Area, cut: &[&Area]) -> Result<Area> {
        let mut args = vec![base.into()];
        args.extend(many(cut));
        self.0.call("areas.difference", &args).map(Area::of)
    }
    pub fn intersect(self, base: &Area, keep: &Area) -> Result<Area> {
        self.0.call("areas.intersect", &[base.into(), keep.into()]).map(Area::of)
    }
    pub fn fill_holes(self, of: &Area) -> Result<Area> {
        self.0.call("areas.fillHoles", &[of.into()]).map(Area::of)
    }
    pub fn grow(self, of: &Area, by: impl Into<Arg>) -> Result<Area> {
        self.0.call("areas.grow", &[of.into(), by.into()]).map(Area::of)
    }
    /// Moved across the page by `[across, down]`.
    pub fn moved(self, of: &Area, by: [impl Into<Arg> + Clone; 2]) -> Result<Area> {
        self.0.call("areas.move", &[of.into(), Arg::from(by)]).map(Area::of)
    }
    pub fn around(self, of: &Features, by: impl Into<Arg>) -> Result<Area> {
        self.0.call("areas.around", &[of.into(), by.into()]).map(Area::of)
    }
    pub fn buffer(self, of: &Lines, by: impl Into<Arg>, options: Obj) -> Result<Area> {
        self.0.call("areas.buffer", &[of.into(), by.into(), Arg::Map(options)]).map(Area::of)
    }
    pub fn shape(self, spec: Obj) -> Result<Area> {
        self.0.call("areas.shape", &[Arg::Map(spec)]).map(Area::of)
    }
    pub fn text(self, spec: Obj) -> Result<Area> {
        self.0.call("areas.text", &[Arg::Map(spec)]).map(Area::of)
    }
    /// A band of the elevation: `{ elevMin, elevMax, depthMin, depthMax }`.
    pub fn band(self, band: Obj, options: Obj) -> Result<Area> {
        self.0.call("areas.band", &[Arg::Map(band), Arg::Map(options)]).map(Area::of)
    }
    pub fn extend_to_frame(self, of: &Area, within: &Area, options: Obj) -> Result<Area> {
        self.0.call("areas.extendToFrame", &[of.into(), within.into(), Arg::Map(options)]).map(Area::of)
    }
    /// The open sea from the tiles, grown to the page's edge.
    pub fn ocean(self) -> Result<Area> {
        self.0.call("engine.ocean", &[]).map(Area::of)
    }
}

impl LinesApi<'_> {
    pub fn of(self, features: &Features) -> Result<Lines> {
        self.0.call("lines.of", &[features.into()]).map(Lines::of)
    }
    pub fn rings(self, of: &Area, options: Obj) -> Result<Lines> {
        self.0.call("lines.rings", &[of.into(), Arg::Map(options)]).map(Lines::of)
    }
    pub fn segments(self, features: &Features) -> Result<Lines> {
        self.0.call("lines.segments", &[features.into()]).map(Lines::of)
    }
    pub fn shape(self, spec: Obj) -> Result<Lines> {
        self.0.call("lines.shape", &[Arg::Map(spec)]).map(Lines::of)
    }
    pub fn text(self, spec: Obj) -> Result<Lines> {
        self.0.call("lines.text", &[Arg::Map(spec)]).map(Lines::of)
    }
    pub fn dedupe(self, of: &Lines) -> Result<Lines> {
        self.0.call("lines.dedupe", &[of.into()]).map(Lines::of)
    }
    pub fn merge(self, of: &Lines) -> Result<Lines> {
        self.0.call("lines.merge", &[of.into()]).map(Lines::of)
    }
    pub fn join(self, sets: &[&Lines]) -> Result<Lines> {
        self.0.call("lines.join", &many(sets)).map(Lines::of)
    }
    pub fn clip(self, of: &Lines, within: &Area, options: Obj) -> Result<Lines> {
        self.0.call("lines.clip", &[of.into(), within.into(), Arg::Map(options)]).map(Lines::of)
    }
    pub fn terrain(self, options: Obj) -> Result<Lines> {
        self.0.call("lines.terrain", &[Arg::Map(options)]).map(Lines::of)
    }
    pub fn contours(self, options: Obj) -> Result<Lines> {
        self.0.call("lines.contours", &[Arg::Map(options)]).map(Lines::of)
    }
    /// A burin document (its JSON, as burin builds it) drawn on the terrain.
    pub fn burin(self, atlas: serde_json::Value, options: Obj) -> Result<Lines> {
        self.0.call("burin.terrain", &[Arg::from_json(&atlas), Arg::Map(options)]).map(Lines::of)
    }
}

impl FillsApi<'_> {
    /// A pattern inside an area: `{ in, pattern, spacing, angle, per, … }`.
    pub fn fill(self, spec: Obj) -> Result<Lines> {
        self.0.call("fills.fill", &[Arg::Map(spec)]).map(Lines::of)
    }
    /// What a fill with the same options covers on paper.
    pub fn cover(self, spec: Obj) -> Result<Area> {
        self.0.call("fills.cover", &[Arg::Map(spec)]).map(Area::of)
    }
}

impl ModulesApi<'_> {
    pub fn roads(self, options: Obj) -> Result<Roads> {
        let out = self.0.call("modules.roads", &[Arg::Map(options)])?;
        let o = out.as_map().cloned().unwrap_or_default();
        let lines = |k: &str| Lines(o.get(k).cloned().unwrap_or(Arg::Null));
        Ok(Roads {
            area: Area(o.get("area").cloned().unwrap_or(Arg::Null)),
            outlines: lines("outlines"),
            details: lines("details"),
            trails: lines("trails"),
            centerlines: lines("centerlines"),
            seams: lines("seams"),
            rails: lines("rails"),
        })
    }
    pub fn labels(self, options: Obj) -> Result<Module> {
        self.0.call("modules.labels", &[Arg::Map(options)]).map(Module)
    }
    pub fn graticule(self, options: Obj) -> Result<Module> {
        self.0.call("modules.graticule", &[Arg::Map(options)]).map(Module)
    }
}

impl Module {
    /// A part as lines: `lines`, a graticule's `labels`, a rule's `lines`.
    pub fn lines(&self, g: &mut Graph, part: &str) -> Result<Lines> {
        g.get(&self.0, part).map(Lines)
    }
    /// A part as an area: `area`, `dots`, a graticule's `keys` and `labelArea`.
    pub fn area(&self, g: &mut Graph, part: &str) -> Result<Area> {
        g.get(&self.0, part).map(Area)
    }
    /// A labels rule's bucket, by its id.
    pub fn rule(&self, g: &mut Graph, id: &str) -> Result<Module> {
        g.get(&self.0, id).map(Module)
    }
}

impl ComposeApi<'_> {
    /// Bottom to top.
    pub fn order(self, ids: &[&str]) -> Result<()> {
        self.0.call("compose.order", &[list(ids)]).map(|_| ())
    }
    /// `{ name, from: area, occludes: [ids] }`.
    pub fn mask(self, spec: Obj) -> Result<()> {
        self.0.call("compose.mask", &[Arg::Map(spec)]).map(|_| ())
    }
    /// `"auto"`, `"none"`, or the areas to clip to.
    pub fn clip(self, clip: impl Into<Arg>) -> Result<()> {
        self.0.call("compose.clip", &[clip.into()]).map(|_| ())
    }
    /// A plotter step: `step` is `deoverlap`, `linemerge`, `simplify`,
    /// `filter`, `reloop`, `hand`, `orient` or `linesort`, on the groups
    /// `layers` (none: every group), with its options (and `except`).
    pub fn step(self, step: &str, layers: &[&str], options: Obj) -> Result<()> {
        let mut args = Vec::new();
        if !layers.is_empty() {
            args.push(list(layers));
        }
        args.push(Arg::Map(options));
        self.0.call(&format!("compose.{step}"), &args).map(|_| ())
    }
}

impl PageApi<'_> {
    pub fn width(self) -> Result<f64> {
        self.0.get_property("page.width").map(|v| v.as_num().unwrap_or(0.0))
    }
    pub fn height(self) -> Result<f64> {
        self.0.get_property("page.height").map(|v| v.as_num().unwrap_or(0.0))
    }
    pub fn center(self) -> Result<[f64; 2]> {
        let w = self.0.get_property("page.width")?.as_num().unwrap_or(0.0);
        let h = self.0.get_property("page.height")?.as_num().unwrap_or(0.0);
        Ok([w / 2.0, h / 2.0])
    }
    /// The paper as an area.
    pub fn frame(self) -> Result<Area> {
        self.0.get_property("page.frame").map(Area)
    }
    /// The ground at `[x, y]` (mm): `(lat, lng)`.
    pub fn to_world(self, p: [f64; 2]) -> Result<(f64, f64)> {
        let v = self.0.call("page.toWorld", &[Arg::from(p)])?;
        let o = v.as_map().cloned().unwrap_or_default();
        Ok((o.get("lat").and_then(Arg::as_num).unwrap_or(0.0), o.get("lng").and_then(Arg::as_num).unwrap_or(0.0)))
    }
}

impl WorldApi<'_> {
    pub fn scale(self) -> Result<f64> {
        self.0.get_property("world.scale").map(|v| v.as_num().unwrap_or(0.0))
    }
    /// Mapbox's zoom at the middle of the page.
    pub fn zoom(self) -> Result<f64> {
        self.0.get_property("world.zoom").map(|v| v.as_num().unwrap_or(0.0))
    }
    /// A Mapbox expression's value at this frame: `["step", ["zoom"], 1, 15, 3]`.
    pub fn at(self, expression: serde_json::Value) -> Result<Arg> {
        self.0.call("world.at", &[Arg::from_json(&expression)])
    }
    pub fn bearing(self) -> Result<f64> {
        self.0.get_property("world.bearing").map(|v| v.as_num().unwrap_or(0.0))
    }
    /// The place at the middle of the page: `(lat, lng)`.
    pub fn center(self) -> Result<(f64, f64)> {
        let o = self.0.get_property("world.center")?.as_map().cloned().unwrap_or_default();
        Ok((o.get("lat").and_then(Arg::as_num).unwrap_or(0.0), o.get("lng").and_then(Arg::as_num).unwrap_or(0.0)))
    }
    /// `(north, south, east, west)` round the page's edge.
    pub fn bounds(self) -> Result<(f64, f64, f64, f64)> {
        let o = self.0.get_property("world.bounds")?.as_map().cloned().unwrap_or_default();
        let n = |k: &str| o.get(k).and_then(Arg::as_num).unwrap_or(0.0);
        Ok((n("north"), n("south"), n("east"), n("west")))
    }
    /// Where the map draws `(lat, lng)`: `[x, y]` in page mm.
    pub fn to_page(self, lat: f64, lng: f64) -> Result<[f64; 2]> {
        let v = self.0.call("world.toPage", &[Arg::Map(crate::obj! { "lat" => lat, "lng" => lng })])?;
        let l = v.as_list().unwrap_or(&[]);
        Ok([l.first().and_then(Arg::as_num).unwrap_or(0.0), l.get(1).and_then(Arg::as_num).unwrap_or(0.0)])
    }
}
