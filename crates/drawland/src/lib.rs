//! draw.land: a profile and a place in, the layered pen-plotter SVG out.
//!
//! This is the whole of what `drawland map` does, as a library: run the
//! profile ([`drawland_script`]), look up the place, download what the graph
//! reads (Overpass, OpenFreeMap's vector tiles, Terrarium elevation, fonts,
//! datasets; cached on disk) and draw it ([`drawland_core`]). The command line,
//! and the Python and Node packages, are this crate.
//!
//! ```no_run
//! use drawland::{Drawer, Place, Profile, Settings};
//!
//! let drawer = Drawer::new(Settings::default());
//! let profile = Profile::read("city-grain.js".as_ref())?;
//! let map = drawer.draw(&profile, &Place::at(37.8771, -122.2562), &())?;
//! map.save("map.svg".as_ref(), Default::default())?;
//! # Ok::<(), String>(())
//! ```

pub mod chrome;
pub mod cli;
mod fonts;
pub mod globe;
mod http;
mod term;
mod serve;
mod view;

pub use drawland_core;
pub use drawland_graph;
pub use drawland_grid;
pub use drawland_core::sheets::{ExportOptions, SliceExport};
pub use drawland_script;
pub use term::Term;

use drawland_core::grammar::Grammar;
use drawland_core::osm::{features_from_overpass, Feature};
use drawland_core::output::Output;
use drawland_core::progress::{thousands, Level, Progress, Task};
use drawland_core::sheets::export_svg_files;
use drawland_core::tiles::{self, Tiles};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The example profile drawn when none is given: parks, water, roads and
/// buildings, in pen.
pub const POSTCARD: &str = include_str!("../postcard.js");

/// Where the map goes, over what the profile's own `output` says: an address
/// (looked up with Nominatim) or a latitude and longitude. Empty: the
/// profile's place.
#[derive(Clone, Debug, Default)]
pub struct Place {
    pub address: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

impl Place {
    pub fn at(lat: f64, lng: f64) -> Place {
        Place { address: None, lat: Some(lat), lng: Some(lng) }
    }

    pub fn address(address: &str) -> Place {
        Place { address: Some(address.to_string()), lat: None, lng: None }
    }
}

/// How the drawer downloads: where it caches, and whether it may use the
/// network.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Overpass replies (`<cache>/overpass/`), Nominatim lookups and datasets.
    /// Vector tiles and elevation are cached by terramask in its own place.
    pub cache: PathBuf,
    /// Only cached replies; never the network for OSM data.
    pub offline: bool,
    /// Download again even when a reply is cached.
    pub refresh: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { cache: PathBuf::from("cache"), offline: false, refresh: false }
    }
}

/// What to draw: a script and the folder it reads beside it (its datasets,
/// SVG sources and the styles its views name: `view({ … }, "city-grain")`
/// reads `city-grain.js` there), or a graph already built.
#[derive(Clone, Debug)]
pub enum Profile {
    /// A script: JavaScript, or TypeScript (its types taken out first).
    Script { source: String, dir: PathBuf, typescript: bool },
    Graph(Box<Grammar>),
}

impl Profile {
    /// A script (`.js`, `.ts`) or a saved graph (`.json`) from a file.
    pub fn read(path: &Path) -> Result<Profile, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = |x: &str| path.extension().is_some_and(|e| e.eq_ignore_ascii_case(x));
        if ext("json") {
            let g = Grammar::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok(Profile::Graph(Box::new(g)));
        }
        Ok(Profile::Script { source: text, dir: dir_of(path), typescript: ext("ts") || ext("mts") })
    }

    /// A script's text (JavaScript); what it reads beside it is read from `dir`.
    pub fn script(source: &str, dir: &Path) -> Profile {
        Profile::Script { source: source.to_string(), dir: dir.to_path_buf(), typescript: false }
    }

    /// A TypeScript script's text.
    pub fn typescript(source: &str, dir: &Path) -> Profile {
        Profile::Script { source: source.to_string(), dir: dir.to_path_buf(), typescript: true }
    }

    /// A graph built in Rust (`drawland_graph::Graph`) or loaded.
    pub fn graph(graph: Grammar) -> Profile {
        Profile::Graph(Box::new(graph))
    }

    /// The built-in postcard, reading beside it from the working directory.
    pub fn postcard() -> Profile {
        Profile::script(POSTCARD, Path::new("."))
    }

    /// The folder the profile reads beside it.
    pub fn dir(&self) -> &Path {
        match self {
            Profile::Script { dir, .. } => dir,
            Profile::Graph(_) => Path::new("."),
        }
    }
}

fn dir_of(path: &Path) -> PathBuf {
    path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf()
}

/// A drawn map: the whole SVG, and the sheets when the output is cut.
#[derive(Clone, Debug)]
pub struct Map {
    pub title: Option<String>,
    pub svg: String,
    /// `(group id, stroke count)` in draw order: a group is a pen.
    pub groups: Vec<(String, usize)>,
    /// `(name, svg)` per sheet when the output is cut (`output.slices`).
    pub slices: Vec<(String, String)>,
}

impl Map {
    /// The files `export` asks for, as `(suffix, svg)`: `""` is the whole map,
    /// `-r1c1` a sheet, `-roads` a layer.
    pub fn files(&self, export: ExportOptions) -> Vec<(String, String)> {
        export_svg_files(&self.svg, &self.slices, export)
    }

    /// Write the files `export` asks for: `out`, and beside it
    /// `<stem><suffix>.<ext>` for each sheet or layer. The paths written.
    pub fn save(&self, out: &Path, export: ExportOptions) -> Result<Vec<PathBuf>, String> {
        let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("map");
        let ext = out.extension().and_then(|s| s.to_str()).unwrap_or("svg");
        let mut written = Vec::new();
        for (suffix, text) in self.files(export) {
            let path = if suffix.is_empty() { out.to_path_buf() } else { out.with_file_name(format!("{stem}{suffix}.{ext}")) };
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            written.push(path);
        }
        Ok(written)
    }
}

/// What a profile makes: a map on paper, or a painted screen.
#[derive(Clone, Debug)]
pub enum Made {
    Paper(Map),
    Screen(Painting),
}

/// A painted screen: what a profile whose output is a screen makes.
#[derive(Clone, Debug)]
pub struct Painting {
    pub title: Option<String>,
    pub picture: drawland_grid::Picture,
}

impl Painting {
    /// Write it to `out`, as its extension says (`png`, `pbm`, `txt`, `ans`,
    /// `html`).
    pub fn save(&self, out: &Path) -> Result<(), String> {
        let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
        let format = drawland_grid::Format::from_extension(ext)
            .ok_or_else(|| format!("{}: a screen is written as .png, .pbm, .txt, .ans or .html", out.display()))?;
        let bytes = self.picture.encode(format, self.title.as_deref().unwrap_or("draw.land"))?;
        std::fs::write(out, bytes).map_err(|e| format!("{}: {e}", out.display()))
    }
}

/// Draws maps, downloading and caching what they read.
pub struct Drawer {
    http: http::Http,
}

impl Drawer {
    pub fn new(settings: Settings) -> Drawer {
        Drawer { http: http::Http::new(settings.cache, settings.offline, settings.refresh) }
    }

    /// `address` looked up with Nominatim (cached): `(lat, lng)`.
    pub fn geocode(&self, address: &str) -> Result<(f64, f64), String> {
        self.http.geocode(address).map(|(lng, lat)| (lat, lng))
    }

    /// The graph `profile` builds at `place`. An address is looked up first,
    /// and so is one the profile's own output names when the script asks
    /// `world` about it.
    pub fn graph(&self, profile: &Profile, place: &Place, progress: &dyn Progress) -> Result<Grammar, String> {
        let (source, typescript) = match profile {
            Profile::Graph(g) => return Ok((**g).clone()),
            Profile::Script { source, typescript, .. } => (source, *typescript),
        };
        let mut lookup = |address: &str| -> Result<(f64, f64), String> {
            let task = Task::begin(progress, "geocode", None, "geocode", 0, "");
            task.note(address);
            let found = self.geocode(address);
            match &found {
                Ok(_) => task.ok(address),
                Err(e) => task.fail(e),
            }
            found
        };
        let at = match (place.lat, place.lng, &place.address) {
            (Some(lat), Some(lng), _) => Some((lat, lng)),
            (_, _, Some(address)) => Some(lookup(address)?),
            _ => None,
        };
        let task = Task::begin(progress, "script", None, "script", 0, "");
        match evaluate(source, typescript, profile.dir(), at, &mut lookup) {
            Ok(g) => {
                let nodes: usize = g.sections.iter().map(|(_, d)| d.len()).sum();
                task.ok(&format!("{nodes} nodes · {} draws", g.draw.len()));
                Ok(g)
            }
            Err(e) => {
                task.fail(&e);
                Err(e)
            }
        }
    }

    /// `profile` drawn at `place`, its progress told to `progress` (`&()`
    /// for none; [`Term`] paints it in a terminal).
    pub fn draw(&self, profile: &Profile, place: &Place, progress: &dyn Progress) -> Result<Map, String> {
        let grammar = self.graph(profile, place, progress)?;
        // The script has the place now, as `world` read it; an address has
        // become coordinates.
        let place = match (place.lat, place.lng, &place.address) {
            (None, None, Some(_)) => Place { address: None, lat: grammar.output.lat, lng: grammar.output.lng },
            _ => place.clone(),
        };
        self.draw_graph(&grammar, &place, profile.dir(), progress)
    }

    /// A graph drawn at `place`; files it names are read from `dir`.
    pub fn draw_graph(&self, grammar: &Grammar, place: &Place, dir: &Path, progress: &dyn Progress) -> Result<Map, String> {
        if grammar.output.screen.is_some() {
            return Err("this profile paints a screen (output.screen), not paper: paint it (Drawer::paint)".into());
        }
        // A page of views needs no place of its own; each view has one.
        let placeless = !grammar.views.is_empty()
            && place.address.is_none()
            && place.lat.is_none()
            && grammar.output.address.is_none()
            && grammar.output.lat.is_none();
        let center = if placeless {
            (0.0, 0.0)
        } else {
            let task = Task::begin(progress, "locate", None, "locate", 0, "");
            let c = self.locate(&task, place, grammar)?;
            task.ok("");
            c
        };
        let output = Output::new(&grammar.output, center);
        let page = self.gather(progress, grammar, &output, dir, "")?;
        let mut views = Vec::new();
        for v in &grammar.views {
            let view_output = Output::new(&v.grammar.output, v.center());
            views.push(self.gather(progress, &v.grammar, &view_output, dir, &format!(" {}", v.id))?);
        }
        let draw = Task::begin(progress, "draw", None, "draw", 0, "");
        let mut inputs = page.inputs();
        inputs.views = views.iter().map(Gathered::inputs).collect();
        let rendered = drawland_core::grammar_run::run_with(grammar, &output, inputs, &draw).map_err(|e| e.to_string())?;
        draw.ok("");
        Ok(Map { title: grammar.title.clone(), svg: rendered.svg, groups: rendered.groups, slices: rendered.slices })
    }

    /// `profile` at `place`, as its output asks: drawn on paper, or painted
    /// on a screen.
    pub fn make(&self, profile: &Profile, place: &Place, progress: &dyn Progress) -> Result<Made, String> {
        let grammar = self.graph(profile, place, progress)?;
        let place = match (place.lat, place.lng, &place.address) {
            (None, None, Some(_)) => Place { address: None, lat: grammar.output.lat, lng: grammar.output.lng },
            _ => place.clone(),
        };
        if grammar.output.screen.is_some() {
            self.paint_graph(&grammar, &place, profile.dir(), progress).map(Made::Screen)
        } else {
            self.draw_graph(&grammar, &place, profile.dir(), progress).map(Made::Paper)
        }
    }

    /// `profile` painted at `place` on the screen its output names
    /// (`output.screen`): a bitmap or a grid of text cells.
    pub fn paint(&self, profile: &Profile, place: &Place, progress: &dyn Progress) -> Result<Painting, String> {
        let grammar = self.graph(profile, place, progress)?;
        let place = match (place.lat, place.lng, &place.address) {
            (None, None, Some(_)) => Place { address: None, lat: grammar.output.lat, lng: grammar.output.lng },
            _ => place.clone(),
        };
        self.paint_graph(&grammar, &place, profile.dir(), progress)
    }

    /// A graph painted at `place` on its screen; files it names are read
    /// from `dir`.
    pub fn paint_graph(&self, grammar: &Grammar, place: &Place, dir: &Path, progress: &dyn Progress) -> Result<Painting, String> {
        self.paint_recalling(grammar, place, dir, progress, &mut drawland_grid::Recall::default())
    }

    /// [`Drawer::paint_graph`] for a screen that moves: each name first
    /// tried where it was on the ground last time (`recall`, kept by the
    /// caller from paint to paint), so names hold still as the map zooms.
    /// Off web Mercator, names are placed afresh.
    pub fn paint_recalling(&self, grammar: &Grammar, place: &Place, dir: &Path, progress: &dyn Progress, recall: &mut drawland_grid::Recall) -> Result<Painting, String> {
        let (output, screen, layers) = self.painted(grammar, place, dir, progress)?;
        let origin = drawland_grid::origin(&output, &screen);
        let picture = match drawland_grid::View::of(&output, screen) {
            Some(view) => recall.render(&view, &layers),
            None => {
                *recall = drawland_grid::Recall::default();
                drawland_grid::render(&screen, origin, &layers)
            }
        };
        Ok(Painting { title: grammar.title.clone(), picture })
    }

    /// A graph's layers at `place` as a [`drawland_grid::Scene`]: its
    /// screen painted wide once, any view of it painted after in a frame's
    /// time. Web Mercator, north up, unbent only.
    pub fn scene_graph(&self, grammar: &Grammar, place: &Place, dir: &Path, progress: &dyn Progress) -> Result<drawland_grid::Scene, String> {
        let (output, screen, layers) = self.painted(grammar, place, dir, progress)?;
        let view = drawland_grid::View::of(&output, screen).ok_or("a screen that moves is web Mercator, north up, with no warp")?;
        if !grammar.output.warp.is_empty() {
            return Err("a screen that moves is web Mercator, north up, with no warp".into());
        }
        Ok(drawland_grid::Scene::new(view, layers))
    }

    /// The graph evaluated for its screen: each `paint` entry's geometry.
    fn painted(&self, grammar: &Grammar, place: &Place, dir: &Path, progress: &dyn Progress) -> Result<(Output, drawland_core::paint::Screen, Vec<drawland_core::grammar_run::Painted>), String> {
        let Some(screen) = grammar.output.screen else {
            return Err("this profile draws paper: give its output a screen, output({ screen: { kind, width, height } })".into());
        };
        let task = Task::begin(progress, "locate", None, "locate", 0, "");
        let center = self.locate(&task, place, grammar)?;
        task.ok("");
        let output = Output::new(&grammar.output, center);
        let page = self.gather(progress, grammar, &output, dir, "")?;
        let draw = Task::begin(progress, "draw", None, "paint", 0, "");
        let layers = drawland_core::grammar_run::paint(grammar, &output, page.inputs(), &draw).map_err(|e| e.to_string())?;
        draw.ok(&format!("{} layers · {} × {}", layers.len(), screen.width, screen.height));
        Ok((output, screen, layers))
    }

    /// The output's centre `(lng, lat)`: the place given, then the profile's.
    fn locate(&self, task: &Task, place: &Place, grammar: &Grammar) -> Result<(f64, f64), String> {
        let center = match (place.address.clone().or(grammar.output.address.clone()), place.lat, place.lng) {
            (_, Some(lat), Some(lng)) => (lng, lat),
            (Some(address), _, _) => {
                let t = task.child("geocode", "geocode", 0, "bytes");
                t.note(&address);
                let center = self.http.geocode(&address)?;
                t.ok(&address);
                center
            }
            (None, _, _) => match (grammar.output.lat, grammar.output.lng) {
                (Some(lat), Some(lng)) => (lng, lat),
                _ => return Err("no location: give an address or a lat/lng, or set the profile's address or lat/lng".into()),
            },
        };
        task.log(Level::Info, &format!("centre {:.7}, {:.7}", center.1, center.0));
        Ok(center)
    }

    /// What `grammar` reads at `output`: Overpass and the vector tiles as it
    /// asks, the terrain when it draws it, its fonts, datasets and SVG
    /// sources. `label` tells the rows of one view from another's.
    fn gather(&self, progress: &dyn Progress, grammar: &Grammar, output: &Output, base: &Path, label: &str) -> Result<Gathered, String> {
        let mut sources = BTreeMap::new();
        for file in grammar.source_files() {
            let path = if Path::new(&file).is_absolute() { PathBuf::from(&file) } else { base.join(&file) };
            sources.insert(file, read(&path)?);
        }
        let id = format!("download{}", label.replace(' ', ":"));
        let download = Task::begin(progress, &id, None, &format!("download{label}"), 0, "");
        download.log(Level::Info, &format!("centre {:.7}, {:.7}", output.center.1, output.center.0));
        let (features, tiles, terrain) = self.fetch(download, output, grammar)?;
        let data = self.read_datasets(progress, grammar, base)?;
        let mut google_fonts = BTreeMap::new();
        for key in drawland_core::labels::google_font_keys_in_grammar(grammar) {
            google_fonts.insert(key.clone(), fonts::fetch_google(&key)?);
        }
        let mut stroke_fonts = BTreeMap::new();
        for key in drawland_core::labels::stroke_font_keys_in_grammar(grammar) {
            stroke_fonts.insert(key.clone(), fonts::fetch_stroke(&key)?);
        }
        Ok(Gathered { features, tiles, terrain, sources, google_fonts, stroke_fonts, data })
    }

    /// The OSM features on `output`, the vector tile features, and the
    /// terrain when the graph draws it: each a row under `download`, which
    /// ends with them.
    fn fetch(&self, download: Task, output: &Output, grammar: &Grammar) -> Result<(Vec<Feature>, Vec<Feature>, Option<drawland_core::terrain::Grid>), String> {
        let area = output.query_polygon();
        let keys = grammar.osm_keys();
        let mut notes: Vec<String> = Vec::new();
        let osm = if keys.is_empty() {
            None
        } else {
            let json = self.http.overpass(output, &keys, &download)?;
            let task = download.child("read", "read OSM", 0, "");
            let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
            let features = features_from_overpass(&json, &area, &keys).map_err(|e| e.to_string())?;
            let found = format!("{} features", thousands(features.len() as u64));
            task.ok(&found);
            notes.push(format!("{:.1} MB · {found}", json.len() as f64 / 1e6));
            Some(features)
        };
        let tile_features = match tiles::plan(output, &grammar.tile_selectors(), osm.as_deref())? {
            Some(t) => read_tiles(&download, t, &mut notes)?,
            None => Vec::new(),
        };
        let terrain = read_terrain(&download, grammar, output)?;
        download.ok(&notes.join(" · "));
        Ok((osm.unwrap_or_default(), tile_features, terrain))
    }

    /// The GeoJSON of every `features.geojson` source: a URL downloaded (and
    /// cached), anything else a file read beside the profile.
    fn read_datasets(&self, progress: &dyn Progress, grammar: &Grammar, base: &Path) -> Result<BTreeMap<String, String>, String> {
        let sources = grammar.datasets();
        let mut out = BTreeMap::new();
        if sources.is_empty() {
            return Ok(out);
        }
        let all = Task::begin(progress, "data", None, "datasets", 0, "");
        for (i, src) in sources.iter().enumerate() {
            let task = all.child(&format!("data-{i}"), &short(src), 0, "");
            let text = if src.starts_with("http://") || src.starts_with("https://") {
                self.http.dataset(src)
            } else {
                let file = src.strip_prefix("file://").unwrap_or(src);
                let path = if Path::new(file).is_absolute() { PathBuf::from(file) } else { base.join(file) };
                read(&path).map(|t| {
                    let note = format!("{:.1} MB · {}", t.len() as f64 / 1e6, path.display());
                    (t, note)
                })
            };
            match text {
                Ok((t, note)) => {
                    task.ok(&note);
                    out.insert(src.clone(), t);
                }
                Err(e) => {
                    task.fail(&e);
                    return Err(e);
                }
            }
        }
        all.ok(&format!("{} datasets", sources.len()));
        Ok(out)
    }
}

/// A script run at `at` (`(lat, lng)`), or where its own output puts it, its
/// views' styles read from `dir`. When the script asks `world` about a place
/// its output names only by an address, the address is looked up and the
/// script run again there.
fn evaluate(
    source: &str,
    typescript: bool,
    dir: &Path,
    at: Option<(f64, f64)>,
    lookup: &mut dyn FnMut(&str) -> Result<(f64, f64), String>,
) -> Result<Grammar, String> {
    let host = drawland_script::Host { place: at, styles: Some(styles_in(dir)), typescript };
    match drawland_script::evaluate_in(source, &host) {
        Err(drawland_script::ScriptError { address: Some(address), .. }) if at.is_none() => {
            let found = lookup(&address)?;
            let host = drawland_script::Host { place: Some(found), ..host };
            drawland_script::evaluate_in(source, &host).map_err(|e| e.to_string())
        }
        other => other.map_err(|e| e.to_string()),
    }
}

/// The styles a view names, read beside the profile: `name.js`, or
/// `name.ts` (its types taken out).
pub fn styles_in(dir: &Path) -> drawland_script::Styles {
    let dir = dir.to_path_buf();
    Rc::new(move |name: &str| {
        let js = dir.join(format!("{name}.js"));
        let ts = dir.join(format!("{name}.ts"));
        match (std::fs::read_to_string(&js), std::fs::read_to_string(&ts)) {
            (Ok(source), _) => Ok(source),
            (Err(_), Ok(source)) => drawland_script::strip_types(&source).map_err(|e| format!("{}: {e}", ts.display())),
            (Err(e), Err(_)) => Err(format!("{}: {e}", js.display())),
        }
    })
}

/// Everything one graph reads, downloaded: what its `Inputs` are made of.
struct Gathered {
    features: Vec<Feature>,
    tiles: Vec<Feature>,
    terrain: Option<drawland_core::terrain::Grid>,
    sources: BTreeMap<String, String>,
    google_fonts: BTreeMap<String, Vec<u8>>,
    stroke_fonts: BTreeMap<String, Vec<u8>>,
    data: BTreeMap<String, String>,
}

impl Gathered {
    fn inputs(&self) -> drawland_core::grammar_run::Inputs<'_> {
        drawland_core::grammar_run::Inputs {
            features: self.features.clone(),
            tiles: self.tiles.clone(),
            sources: &self.sources,
            terrain: self.terrain.clone(),
            memory_budget: None,
            google_fonts: self.google_fonts.clone(),
            stroke_fonts: self.stroke_fonts.clone(),
            data: self.data.clone(),
            views: Vec::new(),
        }
    }
}

/// The elevation under the output, when a def draws it. `None` when none
/// does, so nothing is downloaded for a graph that never asks.
fn read_terrain(download: &Task, grammar: &Grammar, output: &Output) -> Result<Option<drawland_core::terrain::Grid>, String> {
    if !grammar.draws_terrain() {
        return Ok(None);
    }
    let width = drawland_core::terrain::width_px(output);
    let bounds = drawland_core::terrain::bounds_of(output);
    let grid = terramask::Grid::with_width(bounds, width);
    let task = download.child("terrain", "terrain", 0, "tiles");
    // As deep as Terrarium goes (no cap at its zoom 10), so a coast traced
    // from it is fine rather than a pixel staircase.
    let mut fetcher = terramask::Fetcher::with_source(terramask::TERRARIUM);
    fetcher.elevation_max_zoom = terramask::TERRARIUM_MAX_ZOOM;
    let e = fetcher
        .elevation_for(&grid, &terramask::ZoomLimits::default(), |done, total| {
            if done == 1 {
                task.set_total(total as u64);
            }
            task.step(done as u64, "");
        })
        .map_err(|e| format!("terrain: {e}"))?;
    let elev = e.on(&grid);
    task.ok(&format!("zoom {} · {}×{} px", e.zoom(), grid.width, grid.height));
    Ok(Some(drawland_core::terrain::Grid::new(bounds, grid.merc, elev, width)))
}

/// OpenFreeMap's vector tiles under the output (cached by terramask), as the
/// `vector tiles` row; then the features mended across tile edges.
fn read_tiles(download: &Task, mut tiles: Tiles, notes: &mut Vec<String>) -> Result<Vec<Feature>, String> {
    let ids = tiles.tiles(terramask::MAX_ZOOM);
    let n = ids.len() as u64;
    let task = download.child("tiles", "vector tiles", n, "tiles");
    let (plan, warn) = tiles.note(terramask::MAX_ZOOM);
    if warn {
        task.log(Level::Warn, &plan);
    }
    let found = terramask::Fetcher::with_source(tiles::TILES)
        .features_of(&ids, tiles.filter(), |done, total| {
            if done == 1 {
                task.set_total(total as u64);
            }
            task.step(done as u64, "");
        })
        .map_err(|e| format!("vector tiles: {e}"))?;
    tiles.add_features(found);
    task.ok(&format!("zoom {} · {}", tiles.zoom(terramask::MAX_ZOOM), tiles.filter().layers().join(", ")));

    let task = download.child("read-tiles", "read tiles", 0, "");
    let features = tiles.features();
    let found = format!("{} tile features", thousands(features.len() as u64));
    let ocean_n = features.iter().filter(|f| f.tag("class") == Some("ocean")).count();
    task.ok(&if ocean_n > 0 { format!("{found} · {ocean_n} ocean polygons") } else { found.clone() });
    notes.push(format!("{n} tiles · {found}"));
    Ok(features)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// A source's last path segment, for its row.
fn short(src: &str) -> String {
    src.trim_end_matches('/').rsplit('/').next().unwrap_or(src).to_string()
}
