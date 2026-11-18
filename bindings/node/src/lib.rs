//! `require("drawland")`: the draw.land engine for Node. A profile and a
//! place in, the layered SVG out, downloading and caching what the map reads:
//! the same `drawland` crate the command line is. `index.js` wraps these.

use drawland::drawland_graph::{self as dg, Arg};
use drawland::{Drawer, ExportOptions, Place, Profile, Settings, SliceExport, Term};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::cell::RefCell;
use std::path::PathBuf;

/// Where and what to draw, and how to download.
#[napi(object)]
#[derive(Default)]
pub struct DrawOptions {
    /// A profile file: a script (`.js`) or a saved graph (`.json`).
    pub profile: Option<String>,
    /// A script's text, instead of a file.
    pub script: Option<String>,
    /// The script is TypeScript.
    pub typescript: Option<bool>,
    /// A graph's JSON, as `Graph.finish` gives it.
    pub graph: Option<String>,
    /// Where a `script` reads beside it (datasets, styles its views name).
    pub dir: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    /// Looked up with Nominatim; instead of `lat`/`lng`.
    pub address: Option<String>,
    /// The response cache (default `cache`).
    pub cache: Option<String>,
    pub offline: Option<bool>,
    pub refresh: Option<bool>,
    /// Paint the progress on stderr, as the command line does.
    pub progress: Option<bool>,
}

/// Which files to write.
#[napi(object)]
#[derive(Default)]
pub struct ExportFiles {
    /// One file per top-level layer as well: a file per pen.
    pub per_layer: Option<bool>,
    /// `"whole"`, `"slices"` or `"both"` (default), for a cut output.
    pub slices: Option<String>,
    /// Coordinate decimals, 0 to 6.
    pub decimals: Option<u8>,
}

#[napi(object)]
pub struct Group {
    pub id: String,
    pub strokes: u32,
}

#[napi(object)]
pub struct SvgFile {
    /// `""` the whole map, `"-r1c1"` a sheet, `"-roads"` a layer.
    pub suffix: String,
    pub svg: String,
}

fn profile(o: &DrawOptions) -> std::result::Result<Profile, String> {
    let dir = o.dir.as_deref().unwrap_or(".");
    match (&o.profile, &o.script, &o.graph) {
        (_, _, Some(g)) => drawland::drawland_core::grammar::Grammar::from_json(g).map(Profile::graph).map_err(|e| e.to_string()),
        (Some(_), Some(_), _) => Err("give a profile path or a script, not both".into()),
        (Some(p), None, _) => Profile::read(p.as_ref()),
        (None, Some(s), _) if o.typescript.unwrap_or(false) => Ok(Profile::typescript(s, dir.as_ref())),
        (None, Some(s), _) => Ok(Profile::script(s, dir.as_ref())),
        (None, None, _) => Ok(Profile::postcard()),
    }
}

fn place(o: &DrawOptions) -> std::result::Result<Place, String> {
    match (o.lat, o.lng, &o.address) {
        (Some(_), None, _) | (None, Some(_), _) => Err("lat and lng go together".into()),
        (Some(_), Some(_), Some(_)) => Err("an address or a lat/lng, not both".into()),
        _ => Ok(Place { address: o.address.clone(), lat: o.lat, lng: o.lng }),
    }
}

fn settings(o: &DrawOptions, offline: bool) -> Settings {
    Settings {
        cache: PathBuf::from(o.cache.as_deref().unwrap_or("cache")),
        offline: offline || o.offline.unwrap_or(false),
        refresh: o.refresh.unwrap_or(false),
    }
}

fn export(o: Option<ExportFiles>) -> Result<ExportOptions> {
    let o = o.unwrap_or_default();
    let slices = match o.slices.as_deref().unwrap_or("both") {
        "whole" => SliceExport::Whole,
        "slices" => SliceExport::Slices,
        "both" => SliceExport::Both,
        other => return Err(Error::from_reason(format!("slices is \"whole\", \"slices\" or \"both\", got {other:?}"))),
    };
    if o.decimals.is_some_and(|d| d > 6) {
        return Err(Error::from_reason("decimals is 0 to 6"));
    }
    Ok(ExportOptions { decimals: o.decimals, per_layer: o.per_layer.unwrap_or(false), slices })
}

/// A drawn map.
#[napi]
pub struct Map {
    inner: drawland::Map,
}

#[napi]
impl Map {
    /// The profile's `title(...)`, if it set one.
    #[napi(getter)]
    pub fn title(&self) -> Option<String> {
        self.inner.title.clone()
    }

    /// The whole map as one layered SVG, in paper millimetres.
    #[napi(getter)]
    pub fn svg(&self) -> String {
        self.inner.svg.clone()
    }

    /// The groups in draw order, each a pen, with its stroke count.
    #[napi(getter)]
    pub fn groups(&self) -> Vec<Group> {
        self.inner.groups.iter().map(|(id, n)| Group { id: id.clone(), strokes: *n as u32 }).collect()
    }

    /// The sheets when the output is cut (`output.slices`), by name.
    #[napi(getter)]
    pub fn slices(&self) -> Vec<SvgFile> {
        self.inner.slices.iter().map(|(name, svg)| SvgFile { suffix: name.clone(), svg: svg.clone() }).collect()
    }

    /// The files `options` asks for.
    #[napi]
    pub fn files(&self, options: Option<ExportFiles>) -> Result<Vec<SvgFile>> {
        Ok(self.inner.files(export(options)?).into_iter().map(|(suffix, svg)| SvgFile { suffix, svg }).collect())
    }

    /// Write `path`, and beside it a file per sheet or layer; the paths.
    #[napi]
    pub fn save(&self, path: String, options: Option<ExportFiles>) -> Result<Vec<String>> {
        let written = self.inner.save(path.as_ref(), export(options)?).map_err(Error::from_reason)?;
        Ok(written.into_iter().map(|p| p.display().to_string()).collect())
    }
}

pub struct DrawTask {
    options: DrawOptions,
}

impl Task for DrawTask {
    type Output = drawland::Map;
    type JsValue = Map;

    fn compute(&mut self) -> Result<Self::Output> {
        let o = &self.options;
        let run = || -> std::result::Result<drawland::Map, String> {
            let (p, at) = (profile(o)?, place(o)?);
            let drawer = Drawer::new(settings(o, false));
            if o.progress.unwrap_or(false) {
                let term = Term::new();
                let r = drawer.draw(&p, &at, &term);
                term.finish();
                r
            } else {
                drawer.draw(&p, &at, &())
            }
        };
        run().map_err(Error::from_reason)
    }

    fn resolve(&mut self, _: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(Map { inner: output })
    }
}

/// Draw a profile at a place, off the main thread: a Promise of the map.
#[napi(ts_return_type = "Promise<Map>")]
pub fn draw(options: Option<DrawOptions>) -> AsyncTask<DrawTask> {
    AsyncTask::new(DrawTask { options: options.unwrap_or_default() })
}

/// The graph a profile builds, as JSON text: no map data is downloaded.
#[napi]
pub fn graph(options: Option<DrawOptions>) -> Result<String> {
    let o = options.unwrap_or_default();
    let run = || -> std::result::Result<String, String> {
        let (p, at) = (profile(&o)?, place(&DrawOptions { address: None, ..clone_place(&o) })?);
        let drawer = Drawer::new(settings(&o, true));
        drawer.graph(&p, &at, &()).map(|g| g.to_json().to_string())
    };
    run().map_err(Error::from_reason)
}

fn clone_place(o: &DrawOptions) -> DrawOptions {
    DrawOptions { lat: o.lat, lng: o.lng, address: o.address.clone(), ..DrawOptions::default() }
}

// ---------------------------------------------------------------------------
// NativeGraph: the library's calls, for `index.js` to build `Graph` on with
// the interpreter's own `library.js`. Values cross as JSON with handles
// marked (`Arg::encode`).
// ---------------------------------------------------------------------------

fn js_err(e: dg::Error) -> Error {
    Error::from_reason(e.message)
}

fn decode(json: &str) -> Result<Arg> {
    if json.is_empty() {
        return Ok(Arg::Null);
    }
    serde_json::from_str::<serde_json::Value>(json).map(|v| Arg::decode(&v)).map_err(|e| Error::from_reason(e.to_string()))
}

fn list(json: &str) -> Result<Vec<Arg>> {
    Ok(match decode(json)? {
        Arg::List(l) => l,
        Arg::Null => vec![],
        other => vec![other],
    })
}

fn encode(a: &Arg) -> String {
    a.encode().to_string()
}

#[napi]
pub struct NativeGraph {
    inner: RefCell<Option<dg::Graph>>,
    dir: PathBuf,
}

impl NativeGraph {
    fn with<T>(&self, f: impl FnOnce(&mut dg::Graph) -> dg::Result<T>) -> Result<T> {
        let mut g = self.inner.borrow_mut();
        let g = g.as_mut().ok_or_else(|| Error::from_reason("this graph is finished: it was drawn or turned into its graph already"))?;
        f(g).map_err(js_err)
    }
}

#[napi]
impl NativeGraph {
    #[napi(constructor)]
    pub fn new(lat: Option<f64>, lng: Option<f64>, dir: Option<String>) -> Result<Self> {
        let place = match (lat, lng) {
            (Some(lat), Some(lng)) => Some((lat, lng)),
            (None, None) => None,
            _ => return Err(Error::from_reason("lat and lng go together")),
        };
        Ok(NativeGraph { inner: RefCell::new(Some(dg::Graph::new(place))), dir: PathBuf::from(dir.unwrap_or_else(|| ".".into())) })
    }

    #[napi]
    pub fn call(&self, name: String, args: String) -> Result<String> {
        let args = list(&args)?;
        self.with(|g| g.call(&name, &args)).map(|v| encode(&v))
    }

    #[napi]
    pub fn prop(&self, name: String) -> Result<String> {
        self.with(|g| g.get_property(&name)).map(|v| encode(&v))
    }

    #[napi]
    pub fn get(&self, of: String, part: String) -> Result<String> {
        let of = decode(&of)?;
        self.with(|g| g.get(&of, &part)).map(|v| encode(&v))
    }

    #[napi]
    pub fn describe(&self, v: String) -> Result<String> {
        Ok(if v.is_empty() { dg::describe(None) } else { dg::describe(Some(&decode(&v)?)) })
    }

    #[napi]
    pub fn output_begin(&self, spec: String) -> Result<bool> {
        let spec = decode(&spec)?;
        self.with(|g| match &spec {
            Arg::Map(o) => g.output_begin(o),
            other => g.call("output", &[other.clone()]).map(|_| false),
        })
    }

    #[napi]
    pub fn output_warps(&self, done: String) -> Result<()> {
        let done = list(&done)?;
        self.with(|g| g.output_warps(&done))
    }

    #[napi]
    pub fn remap_grid(&self, warp: String, n: u32) -> Result<String> {
        let warp = decode(&warp)?.as_map().cloned().unwrap_or_default();
        self.with(|g| g.remap_grid(&warp, n as usize))
            .map(|(columns, rows, points)| serde_json::json!({ "columns": columns, "rows": rows, "points": points }).to_string())
    }

    #[napi]
    pub fn remap_mesh(&self, n: u32, columns: u32, rows: u32, points: String, went: String) -> Result<String> {
        let points: Vec<[f64; 2]> = serde_json::from_str(&points).map_err(|e| Error::from_reason(e.to_string()))?;
        let went = list(&went)?;
        self.with(|g| g.remap_mesh(n as usize, columns as usize, rows as usize, &points, &went)).map(|v| encode(&v))
    }

    #[napi]
    pub fn view_begin(&self, spec: String) -> Result<()> {
        let spec = decode(&spec)?;
        self.with(|g| g.view_begin(&spec))
    }

    #[napi]
    pub fn view_end(&self, ok: bool) -> Result<String> {
        self.with(|g| g.view_end(ok)).map(|v| encode(&v))
    }

    /// A view drawn in a JavaScript (or TypeScript) style by name, read
    /// beside the graph's `dir`.
    #[napi]
    pub fn view_style(&self, spec: String, name: String) -> Result<String> {
        let spec = decode(&spec)?;
        let styles = drawland::styles_in(&self.dir);
        let source = styles(&name).map_err(|e| Error::from_reason(format!("no style {name:?}: {e}")))?;
        self.with(|g| g.view_begin(&spec))?;
        let graph = self.inner.borrow_mut().take().expect("begun above");
        let id = graph.view_id().unwrap_or_default().to_string();
        let host = drawland::drawland_script::Host { place: None, styles: Some(styles), typescript: false };
        let (mut graph, ran) = drawland::drawland_script::run_into(graph, &source, &host);
        let ended = graph.view_end(ran.is_ok());
        *self.inner.borrow_mut() = Some(graph);
        ran.map_err(|e| Error::from_reason(format!("view {id} ({name}): {e}")))?;
        ended.map(|v| encode(&v)).map_err(js_err)
    }

    /// The graph built, as its JSON. The graph is finished.
    #[napi]
    pub fn finish(&self) -> Result<String> {
        let g = self.inner.borrow_mut().take().ok_or_else(|| Error::from_reason("this graph is finished already"))?;
        g.finish().map(|g| g.to_json().to_string()).map_err(js_err)
    }
}

/// Every function of the library, as a script names it (`areas.union`).
#[napi]
pub fn functions() -> Vec<String> {
    dg::FUNCTIONS.iter().map(|s| s.to_string()).collect()
}

/// The values a script reads rather than calls (`page.width`).
#[napi]
pub fn properties() -> Vec<String> {
    dg::PROPERTIES.iter().map(|s| s.to_string()).collect()
}

/// The library's JavaScript face (`library.js`), the one a profile's
/// interpreter runs: `index.js` builds `Graph` on it.
#[napi]
pub const LIBRARY: &str = drawland::drawland_script::LIBRARY;

/// The command line, `drawland map …`, on `argv` (without the program's
/// name); the exit code.
#[napi]
pub fn cli(argv: Vec<String>) -> i32 {
    drawland::cli::run(std::iter::once("drawland".to_string()).chain(argv))
}

/// The built-in postcard's script: the profile drawn when none is given.
#[napi]
pub const POSTCARD: &str = drawland::POSTCARD;
