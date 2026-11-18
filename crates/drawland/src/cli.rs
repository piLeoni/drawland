//! `drawland map`: the command line, as a function of its arguments, so the
//! binary, `pip install drawland` and `npm install drawland` run the same one.

use crate::{Drawer, ExportOptions, Place, Profile, Settings, SliceExport, Term};
use clap::{Parser, Subcommand, ValueEnum};
use drawland_core::progress::{Event, Level, Progress};
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "drawland", version, about = "draw.land: maps drawn for pen plotters")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum SlicesArg {
    Whole,
    Slices,
    #[default]
    Both,
}

impl From<SlicesArg> for SliceExport {
    fn from(v: SlicesArg) -> Self {
        match v {
            SlicesArg::Whole => SliceExport::Whole,
            SlicesArg::Slices => SliceExport::Slices,
            SlicesArg::Both => SliceExport::Both,
        }
    }
}

const POSTCARD_NAME: &str = "postcard";

#[derive(Subcommand)]
enum Command {
    /// Build the map SVG for a place.
    Map {
        /// Profile: a script (.js) or a saved graph (.json). `postcard`, the
        /// default, is an example built in. A view a script draws in a style
        /// by name (`view({ … }, "city-grain")`) reads `city-grain.js` beside
        /// the profile.
        #[arg(default_value = POSTCARD_NAME)]
        profile: PathBuf,
        /// Print the graph the profile builds, as JSON, and stop.
        #[arg(long)]
        dump_graph: bool,
        /// Street address, looked up with Nominatim. Overrides the profile.
        #[arg(long, conflicts_with_all = ["lat", "lng"])]
        address: Option<String>,
        #[arg(long, requires = "lng", allow_hyphen_values = true)]
        lat: Option<f64>,
        #[arg(long, requires = "lat", allow_hyphen_values = true)]
        lng: Option<f64>,
        #[arg(long = "output", short, default_value = "map.svg")]
        out: PathBuf,
        /// Coordinate decimal places in SVG output (0..6); default is full (4).
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=6))]
        decimals: Option<u8>,
        /// Write one SVG file per top-level layer (`<stem>-<layer>.svg`).
        #[arg(long)]
        per_layer: bool,
        /// Which files to write when the output is cut into slices.
        #[arg(long, value_enum, default_value_t = SlicesArg::Both)]
        slices: SlicesArg,
        /// Response cache: Overpass replies and Nominatim lookups.
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
        /// Use only cached responses; never touch the network for OSM data.
        #[arg(long)]
        offline: bool,
        /// Download again even when the reply is cached.
        #[arg(long, conflicts_with = "offline")]
        refresh: bool,
    },
}

/// The profile named on the command line: a file, or the built-in postcard.
fn profile_at(path: &Path) -> Result<Profile, String> {
    if !path.exists() && path == Path::new(POSTCARD_NAME) {
        return Ok(Profile::postcard());
    }
    Profile::read(path)
}

/// Run the command line on `args` (the program's name first); the exit code.
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() { 2 } else { 0 };
        }
    };
    let Command::Map { profile, dump_graph, address, lat, lng, out, decimals, per_layer, slices, cache, offline, refresh } = cli.command;
    let located = |e: String| format!("{}: {e}", profile.display());
    if dump_graph {
        // No network for a dump: an address comes from the cache or not at all.
        let drawer = Drawer::new(Settings { cache, offline: true, refresh: false });
        let place = Place { address: None, lat, lng };
        let graph = profile_at(&profile).and_then(|p| drawer.graph(&p, &place, &()).map_err(located));
        return match graph {
            Ok(g) => {
                println!("{}", serde_json::to_string_pretty(&g.to_json()).unwrap_or_default());
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
    }
    let export = ExportOptions { decimals, per_layer, slices: slices.into() };
    let term = Term::new();
    let drawer = Drawer::new(Settings { cache, offline, refresh });
    let result = profile_at(&profile)
        .and_then(|p| drawer.draw(&p, &Place { address, lat, lng }, &term).map_err(located))
        .and_then(|map| {
            let written = map.save(&out, export)?;
            let extra = written.len().saturating_sub(1);
            let note = if extra == 0 { out.display().to_string() } else { format!("{} + {extra} files", out.display()) };
            term.event(Event::Log { level: Level::Info, text: &format!("saved {note}") });
            Ok(())
        });
    if let Err(e) = &result {
        term.event(Event::Log { level: Level::Error, text: e });
    }
    term.finish();
    match result {
        Ok(()) => {
            // The file's path for scripts; on a terminal the log says it.
            if !std::io::stdout().is_terminal() {
                println!("{}", out.display());
            }
            0
        }
        Err(_) => 1,
    }
}
