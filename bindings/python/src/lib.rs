//! `import drawland`: draw.land for Python. A profile (a JavaScript or
//! TypeScript file, or a `Graph` built here in Python) and a place in, the
//! layered SVG out, downloading and caching what the map reads: the same
//! `drawland` crate the command line is. `python/drawland/__init__.py` is the
//! package's face; this is what it calls.

use drawland::drawland_graph::{self as dg, Arg, Obj};
use drawland::{Drawer, ExportOptions, Place, Profile, Settings, SliceExport, Term};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use std::cell::RefCell;
use std::path::PathBuf;

pyo3::create_exception!(drawland, Error, PyRuntimeError, "A mistake in a profile, a download that failed, or no place given.");

fn err(message: impl Into<String>) -> PyErr {
    Error::new_err(message.into())
}

fn graph_err(e: dg::Error) -> PyErr {
    err(e.message)
}

// ---------------------------------------------------------------------------
// Values: Python ↔ the library's
// ---------------------------------------------------------------------------

/// A handle from the library: features, an area, lines, or a module's lazy
/// part (which the package wraps so its parts read as attributes).
#[pyclass(name = "Handle", module = "drawland", frozen)]
struct Handle {
    arg: Arg,
}

#[pymethods]
impl Handle {
    /// `"features"`, `"area"`, `"lines"` or `"part"`.
    #[getter]
    fn kind(&self) -> &'static str {
        match self.arg {
            Arg::Features(_) => "features",
            Arg::Area { .. } => "area",
            Arg::Lines { .. } => "lines",
            _ => "part",
        }
    }

    /// The parts a module's lazy part has.
    #[getter]
    fn parts(&self) -> Vec<String> {
        match &self.arg {
            Arg::Lazy(l) => dg::Graph::parts(l),
            _ => vec![],
        }
    }

    /// What the graph calls it (`$areas.polygons_2`).
    #[getter]
    fn reference(&self) -> String {
        match &self.arg {
            Arg::Features(r) | Arg::Area { r, .. } | Arg::Lines { r, .. } => r.clone(),
            _ => String::new(),
        }
    }

    fn __repr__(&self) -> String {
        format!("<drawland {} {}>", self.kind(), self.reference())
    }
}

fn to_arg(v: &Bound<'_, PyAny>) -> PyResult<Arg> {
    if v.is_none() {
        return Ok(Arg::Null);
    }
    if let Ok(h) = v.cast::<Handle>() {
        return Ok(h.get().arg.clone());
    }
    if let Ok(h) = v.getattr("_handle") {
        if let Ok(h) = h.cast::<Handle>() {
            return Ok(h.get().arg.clone());
        }
    }
    if v.is_instance_of::<PyBool>() {
        return Ok(Arg::Bool(v.extract()?));
    }
    if v.is_instance_of::<PyInt>() || v.is_instance_of::<PyFloat>() {
        return Ok(Arg::Num(v.extract()?));
    }
    if let Ok(s) = v.cast::<PyString>() {
        return Ok(Arg::Str(s.extract::<String>()?));
    }
    if let Ok(d) = v.cast::<PyDict>() {
        let mut o = Obj::new();
        for (k, x) in d.iter() {
            o.set(&k.str()?.to_string(), to_arg(&x)?);
        }
        return Ok(Arg::Map(o));
    }
    if v.is_instance_of::<PyList>() || v.is_instance_of::<PyTuple>() {
        return v.try_iter()?.map(|x| to_arg(&x?)).collect::<PyResult<Vec<_>>>().map(Arg::List);
    }
    // A GeoDataFrame, a shapely geometry: GeoJSON by the geo interface.
    if let Ok(geo) = v.getattr("__geo_interface__") {
        return to_arg(&geo);
    }
    // A numpy array or scalar.
    if let Ok(tolist) = v.getattr("tolist") {
        return to_arg(&tolist.call0()?);
    }
    if v.is_callable() {
        return Ok(Arg::Func);
    }
    // A path.
    if let Ok(fspath) = v.getattr("__fspath__") {
        return to_arg(&fspath.call0()?);
    }
    Err(err(format!("drawland can't take {}", v.get_type().name()?)))
}

fn to_py(py: Python<'_>, v: &Arg) -> PyResult<Py<PyAny>> {
    Ok(match v {
        Arg::Null | Arg::Func => py.None(),
        Arg::Bool(b) => b.into_pyobject(py)?.to_owned().into_any().unbind(),
        Arg::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 9e15 {
                (*n as i64).into_pyobject(py)?.into_any().unbind()
            } else {
                n.into_pyobject(py)?.into_any().unbind()
            }
        }
        Arg::Str(s) => s.into_pyobject(py)?.into_any().unbind(),
        Arg::List(l) => {
            let items = l.iter().map(|x| to_py(py, x)).collect::<PyResult<Vec<_>>>()?;
            PyList::new(py, items)?.into_any().unbind()
        }
        Arg::Map(o) => {
            let d = PyDict::new(py);
            for (k, x) in &o.0 {
                d.set_item(k, to_py(py, x)?)?;
            }
            d.into_any().unbind()
        }
        handle => Py::new(py, Handle { arg: handle.clone() })?.into_any(),
    })
}

fn args_of(list: &Bound<'_, PyList>) -> PyResult<Vec<Arg>> {
    list.iter().map(|x| to_arg(&x)).collect()
}

// ---------------------------------------------------------------------------
// Graph: the library, called from Python
// ---------------------------------------------------------------------------

/// A graph built in Python. The package's `drawland.Graph` wraps it with the
/// namespaces (`g.areas.union(…)`); these are the calls it makes.
#[pyclass(name = "_Graph", module = "drawland", unsendable)]
struct PyGraph {
    inner: RefCell<Option<dg::Graph>>,
    place: Option<(f64, f64)>,
    dir: PathBuf,
}

impl PyGraph {
    fn with<T>(&self, f: impl FnOnce(&mut dg::Graph) -> dg::Result<T>) -> PyResult<T> {
        let mut g = self.inner.borrow_mut();
        let g = g.as_mut().ok_or_else(|| err("this graph is finished: it was drawn or turned into its graph already"))?;
        f(g).map_err(graph_err)
    }

    fn take(&self) -> PyResult<dg::Grammar> {
        let g = self.inner.borrow_mut().take().ok_or_else(|| err("this graph is finished: it was drawn or turned into its graph already"))?;
        g.finish().map_err(graph_err)
    }
}

#[pymethods]
impl PyGraph {
    #[new]
    #[pyo3(signature = (*, lat = None, lng = None, dir = PathBuf::from(".")))]
    fn new(lat: Option<f64>, lng: Option<f64>, dir: PathBuf) -> PyResult<Self> {
        let place = match (lat, lng) {
            (Some(lat), Some(lng)) => Some((lat, lng)),
            (None, None) => None,
            _ => return Err(err("lat and lng go together")),
        };
        Ok(PyGraph { inner: RefCell::new(Some(dg::Graph::new(place))), place, dir })
    }

    fn call(&self, py: Python<'_>, name: &str, args: &Bound<'_, PyList>) -> PyResult<Py<PyAny>> {
        let args = args_of(args)?;
        let out = self.with(|g| g.call(name, &args))?;
        to_py(py, &out)
    }

    fn prop(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        let out = self.with(|g| g.get_property(name))?;
        to_py(py, &out)
    }

    fn get(&self, py: Python<'_>, of: &Bound<'_, PyAny>, part: &str) -> PyResult<Py<PyAny>> {
        let of = to_arg(of)?;
        let out = self.with(|g| g.get(&of, part))?;
        to_py(py, &out)
    }

    fn output_begin(&self, spec: &Bound<'_, PyAny>) -> PyResult<bool> {
        let spec = to_arg(spec)?;
        self.with(|g| match &spec {
            Arg::Map(o) => g.output_begin(o),
            other => g.call("output", &[other.clone()]).map(|_| false),
        })
    }

    fn output_warps(&self, done: &Bound<'_, PyList>) -> PyResult<()> {
        let done = args_of(done)?;
        self.with(|g| g.output_warps(&done))
    }

    fn remap_grid(&self, warp: &Bound<'_, PyAny>, n: usize) -> PyResult<(usize, usize, Vec<[f64; 2]>)> {
        let warp = to_arg(warp)?;
        let warp = warp.as_map().cloned().unwrap_or_default();
        self.with(|g| g.remap_grid(&warp, n))
    }

    fn remap_mesh(&self, py: Python<'_>, n: usize, columns: usize, rows: usize, points: Vec<[f64; 2]>, went: &Bound<'_, PyList>) -> PyResult<Py<PyAny>> {
        let went = args_of(went)?;
        let out = self.with(|g| g.remap_mesh(n, columns, rows, &points, &went))?;
        to_py(py, &out)
    }

    fn view_begin(&self, spec: &Bound<'_, PyAny>) -> PyResult<()> {
        let spec = to_arg(spec)?;
        self.with(|g| g.view_begin(&spec))
    }

    fn view_end(&self, py: Python<'_>, ok: bool) -> PyResult<Py<PyAny>> {
        let out = self.with(|g| g.view_end(ok))?;
        to_py(py, &out)
    }

    /// A view drawn in a JavaScript (or TypeScript) style by name, read
    /// beside the graph's `dir`: `city-grain.js`.
    fn view_style(&self, py: Python<'_>, spec: &Bound<'_, PyAny>, name: &str) -> PyResult<Py<PyAny>> {
        let spec = to_arg(spec)?;
        let styles = drawland::styles_in(&self.dir);
        let source = styles(name).map_err(|e| err(format!("no style {name:?}: {e}")))?;
        self.with(|g| g.view_begin(&spec))?;
        let graph = self.inner.borrow_mut().take().expect("begun above");
        let id = graph.view_id().unwrap_or_default().to_string();
        let host = drawland::drawland_script::Host { place: None, styles: Some(styles), typescript: false };
        let (mut graph, ran) = drawland::drawland_script::run_into(graph, &source, &host);
        let ended = graph.view_end(ran.is_ok());
        *self.inner.borrow_mut() = Some(graph);
        ran.map_err(|e| err(format!("view {id} ({name}): {e}")))?;
        to_py(py, &ended.map_err(graph_err)?)
    }

    /// The graph built, as its JSON text. The graph is finished.
    fn finish(&self) -> PyResult<String> {
        Ok(self.take()?.to_json().to_string())
    }

    #[getter]
    fn place(&self) -> Option<(f64, f64)> {
        self.place
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// What to draw: a file, a script's text, a graph built in Python, or the
/// built-in postcard.
fn profile_of(profile: Option<&Bound<'_, PyAny>>, script: Option<String>, typescript: bool, dir: Option<PathBuf>) -> PyResult<Profile> {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    match (profile, script) {
        (Some(_), Some(_)) => Err(err("give a profile or a script, not both")),
        (Some(p), None) => {
            let p = match p.getattr("_g") {
                Ok(inner) if inner.cast::<PyGraph>().is_ok() => inner,
                _ => p.clone(),
            };
            if let Ok(g) = p.cast::<PyGraph>() {
                return Ok(Profile::graph(g.borrow().take()?));
            }
            let path: PathBuf = p.extract().map_err(|_| err("a profile is a path to a .js, .ts or .json file, or a drawland.Graph"))?;
            Profile::read(&path).map_err(err)
        }
        (None, Some(s)) if typescript => Ok(Profile::typescript(&s, &dir)),
        (None, Some(s)) => Ok(Profile::script(&s, &dir)),
        (None, None) => Ok(Profile::postcard()),
    }
}

fn place(lat: Option<f64>, lng: Option<f64>, address: Option<String>) -> PyResult<Place> {
    match (lat, lng) {
        (Some(_), None) | (None, Some(_)) => Err(err("lat and lng go together")),
        _ if address.is_some() && lat.is_some() => Err(err("an address or a lat/lng, not both")),
        _ => Ok(Place { address, lat, lng }),
    }
}

fn export(per_layer: bool, slices: &str, decimals: Option<u8>) -> PyResult<ExportOptions> {
    let slices = match slices {
        "whole" => SliceExport::Whole,
        "slices" => SliceExport::Slices,
        "both" => SliceExport::Both,
        other => return Err(err(format!("slices is \"whole\", \"slices\" or \"both\", got {other:?}"))),
    };
    if decimals.is_some_and(|d| d > 6) {
        return Err(err("decimals is 0 to 6"));
    }
    Ok(ExportOptions { decimals, per_layer, slices })
}

/// A drawn map.
#[pyclass(module = "drawland", frozen)]
struct Map {
    inner: drawland::Map,
}

#[pymethods]
impl Map {
    /// The profile's `title(...)`, if it set one.
    #[getter]
    fn title(&self) -> Option<String> {
        self.inner.title.clone()
    }

    /// The whole map as one layered SVG, in paper millimetres.
    #[getter]
    fn svg(&self) -> String {
        self.inner.svg.clone()
    }

    /// `(group id, strokes)` in draw order: a group is a pen.
    #[getter]
    fn groups(&self) -> Vec<(String, usize)> {
        self.inner.groups.clone()
    }

    /// `{name: svg}` per sheet when the output is cut (`output.slices`).
    #[getter]
    fn slices<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        for (name, svg) in &self.inner.slices {
            d.set_item(name, svg)?;
        }
        Ok(d)
    }

    /// `{suffix: svg}`: `""` the whole map, `"-r1c1"` a sheet, `"-roads"` a
    /// layer (with `per_layer`).
    #[pyo3(signature = (*, per_layer = false, slices = "both", decimals = None))]
    fn files<'py>(&self, py: Python<'py>, per_layer: bool, slices: &str, decimals: Option<u8>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        for (suffix, svg) in self.inner.files(export(per_layer, slices, decimals)?) {
            d.set_item(suffix, svg)?;
        }
        Ok(d)
    }

    /// Write `path`, and beside it a file per sheet or layer; the paths.
    #[pyo3(signature = (path, *, per_layer = false, slices = "both", decimals = None))]
    fn save(&self, path: PathBuf, per_layer: bool, slices: &str, decimals: Option<u8>) -> PyResult<Vec<PathBuf>> {
        self.inner.save(&path, export(per_layer, slices, decimals)?).map_err(err)
    }

    fn __repr__(&self) -> String {
        let strokes: usize = self.inner.groups.iter().map(|(_, n)| n).sum();
        format!(
            "<drawland.Map {:?}: {} groups, {strokes} strokes>",
            self.inner.title.as_deref().unwrap_or("untitled"),
            self.inner.groups.len()
        )
    }
}

/// Draw a profile at a place.
#[pyfunction]
#[pyo3(signature = (profile = None, *, script = None, typescript = false, dir = None, lat = None, lng = None, address = None, cache = PathBuf::from("cache"), offline = false, refresh = false, progress = false))]
#[allow(clippy::too_many_arguments)]
fn draw(
    py: Python<'_>,
    profile: Option<&Bound<'_, PyAny>>,
    script: Option<String>,
    typescript: bool,
    dir: Option<PathBuf>,
    lat: Option<f64>,
    lng: Option<f64>,
    address: Option<String>,
    cache: PathBuf,
    offline: bool,
    refresh: bool,
    progress: bool,
) -> PyResult<Map> {
    let at = place(lat, lng, address)?;
    let p = profile_of(profile, script, typescript, dir)?;
    let map = py.detach(move || {
        let drawer = Drawer::new(Settings { cache, offline, refresh });
        if progress {
            let term = Term::new();
            let r = drawer.draw(&p, &at, &term);
            term.finish();
            r
        } else {
            drawer.draw(&p, &at, &())
        }
    });
    map.map(|inner| Map { inner }).map_err(err)
}

/// The graph a profile builds, as JSON text: no map data is downloaded.
#[pyfunction]
#[pyo3(signature = (profile = None, *, script = None, typescript = false, dir = None, lat = None, lng = None, cache = PathBuf::from("cache")))]
#[allow(clippy::too_many_arguments)]
fn graph(
    py: Python<'_>,
    profile: Option<&Bound<'_, PyAny>>,
    script: Option<String>,
    typescript: bool,
    dir: Option<PathBuf>,
    lat: Option<f64>,
    lng: Option<f64>,
    cache: PathBuf,
) -> PyResult<String> {
    let at = place(lat, lng, None)?;
    let p = profile_of(profile, script, typescript, dir)?;
    py.detach(move || {
        let drawer = Drawer::new(Settings { cache, offline: true, refresh: false });
        drawer.graph(&p, &at, &()).map(|g| g.to_json().to_string())
    })
    .map_err(err)
}

/// The command line, `drawland map …`, on `argv` (without the program's
/// name); the exit code.
#[pyfunction]
fn cli(py: Python<'_>, argv: Vec<String>) -> i32 {
    py.detach(move || drawland::cli::run(std::iter::once("drawland".to_string()).chain(argv)))
}

/// Every function of the library, as a script names it (`areas.union`).
#[pyfunction]
fn functions() -> Vec<&'static str> {
    dg::FUNCTIONS.to_vec()
}

/// The values a script reads rather than calls (`page.width`).
#[pyfunction]
fn properties() -> Vec<&'static str> {
    dg::PROPERTIES.to_vec()
}

#[pymodule]
fn _drawland(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("POSTCARD", drawland::POSTCARD)?;
    m.add("Error", m.py().get_type::<Error>())?;
    m.add_class::<Map>()?;
    m.add_class::<Handle>()?;
    m.add_class::<PyGraph>()?;
    m.add_function(wrap_pyfunction!(draw, m)?)?;
    m.add_function(wrap_pyfunction!(graph, m)?)?;
    m.add_function(wrap_pyfunction!(cli, m)?)?;
    m.add_function(wrap_pyfunction!(functions, m)?)?;
    m.add_function(wrap_pyfunction!(properties, m)?)?;
    Ok(())
}
