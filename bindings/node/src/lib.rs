//! `require("drawland")`: the draw.land engine for Node. A profile and a
//! place in, the layered SVG out, downloading and caching what the map reads:
//! the same `drawland` crate the command line is. `index.js` wraps these.

use drawland::{Drawer, ExportOptions, Place, Profile, Settings, SliceExport, Term};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::path::PathBuf;

/// Where and what to draw, and how to download.
#[napi(object)]
#[derive(Default)]
pub struct DrawOptions {
    /// A profile file: a script (`.js`) or a saved graph (`.json`).
    pub profile: Option<String>,
    /// A script's text, instead of a file.
    pub script: Option<String>,
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
    match (&o.profile, &o.script) {
        (Some(_), Some(_)) => Err("give a profile path or a script, not both".into()),
        (Some(p), None) => Profile::read(p.as_ref()),
        (None, Some(s)) => Ok(Profile::script(s, o.dir.as_deref().unwrap_or(".").as_ref())),
        (None, None) => Ok(Profile::postcard()),
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

/// The command line, `drawland map …`, on `argv` (without the program's
/// name); the exit code.
#[napi]
pub fn cli(argv: Vec<String>) -> i32 {
    drawland::cli::run(std::iter::once("drawland".to_string()).chain(argv))
}

/// The built-in postcard's script: the profile drawn when none is given.
#[napi]
pub const POSTCARD: &str = drawland::POSTCARD;
