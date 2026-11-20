//! `drawland serve`: a screen profile's map in a web page on this machine,
//! moved as smoothly as a frame can be painted.
//!
//! The graph is run once for an area wider than the screen (a
//! [`drawland_grid::Scene`]); each frame (`/frame?lng&lat&scale&w&h`) is that
//! scene's geometry moved, zoomed and painted, which takes milliseconds. A
//! frame that nears the scene's edge, or zooms half a step from its scale,
//! asks for a new scene, built on this (the main) thread while frames go on
//! being painted from the old one; its vector tiles are as deep as the
//! view's detail ([`View::tile_zoom`]), so a scene far out is as quick as one
//! near. A frame moves the map by whole cells and says (`X-Shift`) how far
//! that leaves it off the view, for the page to make up. A bitmap frame is a PBM (1 bit a pixel),
//! a text frame each row's runs of one colour as JSON. One profile a port,
//! so a 1-bit and a text site run side by side, each its own page.

use crate::view::open;
use crate::{Drawer, Place, Profile};
use drawland_core::grammar::Grammar;
use drawland_core::paint::{Screen, ScreenKind};
use drawland_grid::{Picture, Scene, View};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const PAGE: &str = include_str!("serve.html");

/// A scene is this many screens across and down, the view in its middle.
const WIDE: f64 = 3.0;

/// The scene frames are painted from, and the last build's failure.
#[derive(Default)]
struct Shared {
    scene: Option<Arc<Scene>>,
    error: Option<String>,
    building: bool,
}

type State = Arc<(Mutex<Shared>, Condvar)>;

pub fn run(drawer: &Drawer, profile: &Profile, place: &Place, port: u16) -> Result<(), String> {
    let (grammar, screen, (lng, lat)) = open(drawer, profile, place, "serve")?;
    let title = grammar.title.clone().unwrap_or_else(|| "draw.land".into());
    let kind = match screen.kind {
        ScreenKind::Bitmap => "bitmap",
        ScreenKind::Text => "text",
    };
    let start = serde_json::json!({
        "title": title, "kind": kind, "lat": lat, "lng": lng,
        "scale": grammar.output.scale.0, "width": screen.width, "height": screen.height, "invert": screen.invert,
    });
    let page = PAGE.replace("/*START*/null", &start.to_string()).replace("<title>draw.land</title>", &format!("<title>{}</title>", escape(&title)));
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    eprintln!("{title}: http://localhost:{port}/  ({kind}; Ctrl-C stops)");

    let state: State = Arc::default();
    let (want, wanted) = mpsc::channel::<View>();
    {
        let state = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (state, want, page) = (state.clone(), want.clone(), page.clone());
                std::thread::spawn(move || serve(stream, &state, &want, &page, screen));
            }
        });
    }
    // Scenes are built here, the newest view asked for first.
    while let Ok(mut view) = wanted.recv() {
        while let Ok(newer) = wanted.try_recv() {
            view = newer;
        }
        let (lock, ready) = &*state;
        {
            let mut shared = lock.lock().unwrap();
            if shared.scene.as_ref().is_some_and(|s| s.covers(&view)) {
                shared.building = false;
                continue;
            }
            shared.building = true;
        }
        let t = Instant::now();
        let built = build(drawer, profile, &grammar, &view);
        let mut shared = lock.lock().unwrap();
        match built {
            Ok(scene) => {
                eprintln!("  scene {} × {} in {} ms", scene.view().screen.width, scene.view().screen.height, t.elapsed().as_millis());
                shared.scene = Some(Arc::new(scene));
                shared.error = None;
            }
            Err(e) => {
                eprintln!("  {e}");
                shared.error = Some(e);
            }
        }
        shared.building = false;
        ready.notify_all();
    }
    Ok(())
}

/// The scene for `view`: the graph run at its centre and scale on a screen
/// [`WIDE`] times its size, its tiles at the view's zoom.
fn build(drawer: &Drawer, profile: &Profile, grammar: &Grammar, view: &View) -> Result<Scene, String> {
    let (lng, lat) = drawland_grid::scene::geographic(view.center);
    let mut grammar = grammar.clone();
    let mut screen = view.screen;
    screen.width = ((screen.width as f64 * WIDE) as u32).min(8192);
    screen.height = ((screen.height as f64 * WIDE) as u32).min(8192);
    grammar.output.screen = Some(screen);
    grammar.output.scale.0 = view.per_unit * 1000.0 * lat.to_radians().cos();
    // Tiles as deep as the view's detail, not as deep as a cap allows: far
    // out, a scene reads a few dozen shallow tiles, not a thousand deep ones.
    grammar.output.tile_zoom = Some(view.tile_zoom());
    grammar.output.fit_screen()?;
    drawer.scene_graph(&grammar, &Place::at(lat, lng), profile.dir(), &())
}

/// One connection's requests, kept alive.
fn serve(stream: TcpStream, state: &State, want: &mpsc::Sender<View>, page: &str, screen: Screen) {
    let Ok(mut out) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    while let Some(target) = request(&mut reader) {
        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        let reply = match path {
            "/" => Reply::ok("text/html; charset=utf-8", page.as_bytes().to_vec()),
            "/frame" => frame(state, want, screen, query).unwrap_or_else(|e| Reply { status: "422 Unprocessable Content", kind: "text/plain; charset=utf-8", body: e.into_bytes(), scene: "", shift: String::new() }),
            _ => Reply { status: "404 Not Found", kind: "text/plain", body: b"not here".to_vec(), scene: "", shift: String::new() },
        };
        if reply.send(&mut out).is_err() {
            return;
        }
    }
}

/// The view the query asks for, painted from the scene there is; a new
/// scene asked for when this one will not hold it much longer.
fn frame(state: &State, want: &mpsc::Sender<View>, screen: Screen, query: &str) -> Result<Reply, String> {
    let number = |key: &str| -> Result<f64, String> {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
            .ok_or_else(|| format!("{key} is missing"))?
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{key} is not a number"))
    };
    let mut s = screen;
    s.width = (number("w")? as u32).clamp(1, 4096);
    s.height = (number("h")? as u32).clamp(1, 4096);
    let view = View::at(number("lng")?, number("lat")?, number("scale")?.clamp(100.0, 1e8), s);
    let (lock, ready) = &**state;
    let mut shared = lock.lock().unwrap();
    if !shared.scene.as_ref().is_some_and(|sc| sc.covers(&view)) && !shared.building {
        let _ = want.send(view);
        shared.building = true;
    }
    // The first scene is waited for; after that, the old one is painted.
    let deadline = Instant::now() + Duration::from_secs(120);
    while shared.scene.is_none() {
        if let Some(e) = shared.error.take() {
            return Err(e);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("no scene yet".into());
        }
        shared = ready.wait_timeout(shared, left).unwrap().0;
    }
    let scene = shared.scene.clone().unwrap();
    let building = shared.building;
    drop(shared);
    let (picture, (dx, dy)) = scene.render(&view);
    let body = match &picture {
        Picture::Bitmap(b) => b.pbm(),
        Picture::Text(t) => text_runs(t).into_bytes(),
    };
    let kind = if matches!(picture, Picture::Bitmap(_)) { "image/x-portable-bitmap" } else { "application/json" };
    Ok(Reply { status: "200 OK", kind, body, scene: if building { "building" } else { "ready" }, shift: format!("{dx:.4},{dy:.4}") })
}

/// Text cells as JSON: each row a list of `[text, fg, bg]` runs of one colour.
fn text_runs(t: &drawland_grid::Text) -> String {
    let colour = |c: Option<drawland_core::paint::Rgb>| c.map(|c| serde_json::Value::String(c.hex())).unwrap_or(serde_json::Value::Null);
    let rows: Vec<serde_json::Value> = (0..t.height)
        .map(|y| {
            let mut runs = Vec::new();
            let mut x = 0;
            while x < t.width {
                let c = *t.cell(x, y);
                let mut run = String::new();
                while x < t.width && t.cell(x, y).fg == c.fg && t.cell(x, y).bg == c.bg {
                    run.push(t.cell(x, y).char());
                    x += 1;
                }
                runs.push(serde_json::json!([run, colour(c.fg), colour(c.bg)]));
            }
            serde_json::Value::Array(runs)
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

/// The next request's target (`/frame?…`), its headers and body read past;
/// `None` when the connection closes. Anything but GET is answered as GET.
fn request(reader: &mut BufReader<TcpStream>) -> Option<String> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let target = line.split_whitespace().nth(1)?.to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            return None;
        }
        if header.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = header.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(target)
}

struct Reply {
    status: &'static str,
    kind: &'static str,
    body: Vec<u8>,
    /// Whether a newer scene is on its way (`building`) or not (`ready`).
    scene: &'static str,
    /// How far the picture is off the view asked for, in units (`dx,dy`).
    shift: String,
}

impl Reply {
    fn ok(kind: &'static str, body: Vec<u8>) -> Reply {
        Reply { status: "200 OK", kind, body, scene: "", shift: String::new() }
    }

    fn send(self, stream: &mut TcpStream) -> std::io::Result<()> {
        let head = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Scene: {}\r\nX-Shift: {}\r\n\r\n",
            self.status,
            self.kind,
            self.body.len(),
            self.scene,
            self.shift
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
