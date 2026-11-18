//! `import drawland`: the draw.land engine for Python. A profile and a place
//! in, the layered SVG out, downloading and caching what the map reads: the
//! same `drawland` crate the command line is.

use drawland::{Drawer, ExportOptions, Place, Profile, Settings, SliceExport, Term};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::PathBuf;

/// What to draw: a file, a script's text, or the built-in postcard.
fn profile(path: Option<PathBuf>, script: Option<String>, dir: Option<PathBuf>) -> Result<Profile, String> {
    match (path, script) {
        (Some(_), Some(_)) => Err("give a profile path or a script, not both".into()),
        (Some(p), None) => Profile::read(&p),
        (None, Some(s)) => Ok(Profile::script(&s, &dir.unwrap_or_else(|| PathBuf::from(".")))),
        (None, None) => Ok(Profile::postcard()),
    }
}

fn place(lat: Option<f64>, lng: Option<f64>, address: Option<String>) -> PyResult<Place> {
    match (lat, lng) {
        (Some(_), None) | (None, Some(_)) => Err(PyValueError::new_err("lat and lng go together")),
        _ if address.is_some() && lat.is_some() => Err(PyValueError::new_err("an address or a lat/lng, not both")),
        _ => Ok(Place { address, lat, lng }),
    }
}

fn export(per_layer: bool, slices: &str, decimals: Option<u8>) -> PyResult<ExportOptions> {
    let slices = match slices {
        "whole" => SliceExport::Whole,
        "slices" => SliceExport::Slices,
        "both" => SliceExport::Both,
        other => return Err(PyValueError::new_err(format!("slices is \"whole\", \"slices\" or \"both\", got {other:?}"))),
    };
    if decimals.is_some_and(|d| d > 6) {
        return Err(PyValueError::new_err("decimals is 0 to 6"));
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
        self.inner.save(&path, export(per_layer, slices, decimals)?).map_err(PyRuntimeError::new_err)
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
#[pyo3(signature = (profile = None, *, script = None, dir = None, lat = None, lng = None, address = None, cache = PathBuf::from("cache"), offline = false, refresh = false, progress = false))]
#[allow(clippy::too_many_arguments)]
fn draw(
    py: Python<'_>,
    profile: Option<PathBuf>,
    script: Option<String>,
    dir: Option<PathBuf>,
    lat: Option<f64>,
    lng: Option<f64>,
    address: Option<String>,
    cache: PathBuf,
    offline: bool,
    refresh: bool,
    progress: bool,
) -> PyResult<Map> {
    let place = place(lat, lng, address)?;
    let map = py.detach(move || {
        let p = self::profile(profile, script, dir)?;
        let drawer = Drawer::new(Settings { cache, offline, refresh });
        if progress {
            let term = Term::new();
            let r = drawer.draw(&p, &place, &term);
            term.finish();
            r
        } else {
            drawer.draw(&p, &place, &())
        }
    });
    map.map(|inner| Map { inner }).map_err(PyRuntimeError::new_err)
}

/// The graph a profile builds, as JSON text: no map data is downloaded.
#[pyfunction]
#[pyo3(signature = (profile = None, *, script = None, dir = None, lat = None, lng = None, cache = PathBuf::from("cache")))]
fn graph(
    py: Python<'_>,
    profile: Option<PathBuf>,
    script: Option<String>,
    dir: Option<PathBuf>,
    lat: Option<f64>,
    lng: Option<f64>,
    cache: PathBuf,
) -> PyResult<String> {
    let place = place(lat, lng, None)?;
    py.detach(move || {
        let p = self::profile(profile, script, dir)?;
        let drawer = Drawer::new(Settings { cache, offline: true, refresh: false });
        drawer.graph(&p, &place, &()).map(|g| g.to_json().to_string())
    })
    .map_err(PyRuntimeError::new_err)
}

/// The command line, `drawland map …`, on `argv` (without the program's
/// name); the exit code.
#[pyfunction]
fn cli(py: Python<'_>, argv: Vec<String>) -> i32 {
    py.detach(move || drawland::cli::run(std::iter::once("drawland".to_string()).chain(argv)))
}

#[pymodule]
fn _drawland(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("POSTCARD", drawland::POSTCARD)?;
    m.add_class::<Map>()?;
    m.add_function(wrap_pyfunction!(draw, m)?)?;
    m.add_function(wrap_pyfunction!(graph, m)?)?;
    m.add_function(wrap_pyfunction!(cli, m)?)?;
    Ok(())
}
