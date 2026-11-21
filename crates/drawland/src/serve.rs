//! `drawland serve`: a screen profile's map in a web page on this machine,
//! moved as smoothly as a frame can be painted, with the interface round it
//! drawn in the screen's own medium ([`crate::chrome`]).
//!
//! The graph is run once for an area wider than the screen (a
//! [`drawland_grid::Scene`]); each frame (`/frame?style&lng&lat&scale&w&h`)
//! is that scene's geometry moved, zoomed and painted, which takes
//! milliseconds. A frame that nears the scene's edge, or zooms half a step
//! from its scale, asks for a new scene, built on this (the main) thread
//! while frames go on being painted from the old one; its vector tiles are
//! as deep as the view's detail ([`View::tile_zoom`]), so a scene far out is
//! as quick as one near. A frame moves the map by whole cells and says
//! (`X-Shift`) how far that leaves it off the view (under half a cell: the
//! page draws it where it is). A globe's frame says `X-Globe`: the page never
//! moves it, a globe turns.
//! A bitmap frame is a PBM (1 bit a pixel), a text frame each row's runs of
//! one colour as JSON.
//!
//! The interface (`/chrome?…`) is laid out here from the page's state and
//! sent apart from the map, so the map moves under it: on text, runs of
//! cells; on a bitmap, its pixels and where it covers the map; each with the
//! zones a press acts on. The styles are the screen profiles of the same
//! kind beside the one served; `/source?style` reads one and a POST there
//! writes it and draws it again, for the editor page (`/edit?style`).

use crate::chrome::{self, Host, Now, Ui};
use crate::view::open;
use crate::{Drawer, Place, Profile};
use drawland_core::grammar::Grammar;
use drawland_core::paint::{Screen, ScreenKind};
use drawland_grid::ui::{bitmap_font, Chrome, Palette, Pixels};
use drawland_grid::{Picture, Recall, Scene, View};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const PAGE: &str = include_str!("serve.html");
const EDIT: &str = include_str!("edit.html");

/// A scene is this many screens across and down, the view in its middle.
const WIDE: f64 = 3.0;

/// One style served: its profile and graph, and the scene frames are
/// painted from.
struct Style {
    name: String,
    path: PathBuf,
    inner: Mutex<Inner>,
    ready: Condvar,
}

struct Inner {
    profile: Profile,
    grammar: Grammar,
    screen: Screen,
    scene: Option<Arc<Scene>>,
    /// Far out, the last globe painted and the view it shows.
    globe: Option<(Ask, Arc<Picture>)>,
    /// Where the names were on each page's last frame (by its `who`), so
    /// they hold still as its map zooms.
    recall: HashMap<String, Recall>,
    error: Option<String>,
    building: bool,
    /// Counts the profile's readings: a scene built from an older one is
    /// dropped.
    generation: u64,
}

type Styles = Arc<Vec<Style>>;

/// A view asked for, and (on a globe) whether its names run past the rim.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ask {
    view: View,
    names_past_rim: bool,
}

pub fn run(drawer: &Drawer, path: &Path, place: &Place, port: u16) -> Result<(), String> {
    let profile = Profile::read(path)?;
    let (grammar, screen, (lng, lat)) = open(drawer, &profile, place, "serve")?;
    let kind = screen.kind;
    let mut list = vec![Style {
        name: name_of(path),
        path: path.to_path_buf(),
        inner: Mutex::new(Inner { profile, grammar: grammar.clone(), screen, scene: None, globe: None, recall: HashMap::new(), error: None, building: false, generation: 0 }),
        ready: Condvar::new(),
    }];
    for s in chrome::styles(path, Some(kind)).into_iter().skip(1) {
        let read = Profile::read(&s.path).and_then(|p| {
            let (g, sc, _) = open(drawer, &p, &Place::at(lat, lng), "serve")?;
            Ok((p, g, sc))
        });
        match read {
            Ok((profile, grammar, screen)) if screen.kind == kind => list.push(Style {
                name: s.name,
                path: s.path,
                inner: Mutex::new(Inner { profile, grammar, screen, scene: None, globe: None, recall: HashMap::new(), error: None, building: false, generation: 0 }),
                ready: Condvar::new(),
            }),
            Ok(_) => {}
            Err(e) => eprintln!("  {}: {e}", s.path.display()),
        }
    }
    let styles: Styles = Arc::new(list);

    let title = grammar.title.clone().unwrap_or_else(|| "draw.land".into());
    let kind_name = match kind {
        ScreenKind::Bitmap => "bitmap",
        ScreenKind::Text => "text",
    };
    let start = serde_json::json!({
        "title": title, "kind": kind_name, "lat": lat, "lng": lng,
        "scale": grammar.output.scale.0, "width": screen.width, "height": screen.height, "invert": screen.invert,
        "style": styles[0].name,
    });
    let page = PAGE.replace("/*START*/null", &start.to_string()).replace("<title>draw.land</title>", &format!("<title>{}</title>", escape(&title)));
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
    eprintln!("{title}: http://localhost:{port}/  ({kind_name}: {}; Ctrl-C stops)", names.join(", "));

    let (want, wanted) = mpsc::channel::<(usize, Ask)>();
    {
        let styles = styles.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (styles, want, page) = (styles.clone(), want.clone(), page.clone());
                std::thread::spawn(move || serve(stream, &styles, &want, &page));
            }
        });
    }
    // Scenes are built here, the newest view asked for first.
    let mut pending: Vec<Option<Ask>> = vec![None; styles.len()];
    while let Ok((i, ask)) = wanted.recv() {
        pending[i] = Some(ask);
        while let Ok((i, newer)) = wanted.try_recv() {
            pending[i] = Some(newer);
        }
        for i in 0..styles.len() {
            let Some(ask) = pending[i].take() else { continue };
            let view = ask.view;
            let style = &styles[i];
            let far = crate::globe::far_view(&view);
            let (grammar, profile, generation) = {
                let mut inner = style.inner.lock().unwrap();
                let done = if far { inner.globe.as_ref().is_some_and(|(a, _)| *a == ask) } else { inner.scene.as_ref().is_some_and(|s| s.covers(&view)) };
                if done {
                    inner.building = false;
                    continue;
                }
                inner.building = true;
                (inner.grammar.clone(), inner.profile.clone(), inner.generation)
            };
            let t = Instant::now();
            if far {
                let painted = globe(drawer, &profile, &grammar, &ask);
                let mut inner = style.inner.lock().unwrap();
                if inner.generation == generation {
                    match painted {
                        Ok(p) => {
                            eprintln!("  {}: globe in {} ms", style.name, t.elapsed().as_millis());
                            inner.globe = Some((ask, Arc::new(p)));
                            inner.error = None;
                        }
                        Err(e) => {
                            eprintln!("  {}: {e}", style.name);
                            inner.error = Some(e);
                        }
                    }
                }
                inner.building = false;
                style.ready.notify_all();
                continue;
            }
            let built = build(drawer, &profile, &grammar, &view);
            let mut inner = style.inner.lock().unwrap();
            if inner.generation == generation {
                match built {
                    Ok(scene) => {
                        eprintln!("  {}: scene {} × {} in {} ms", style.name, scene.view().screen.width, scene.view().screen.height, t.elapsed().as_millis());
                        inner.scene = Some(Arc::new(scene));
                        inner.error = None;
                    }
                    Err(e) => {
                        eprintln!("  {}: {e}", style.name);
                        inner.error = Some(e);
                    }
                }
            }
            inner.building = false;
            style.ready.notify_all();
        }
    }
    Ok(())
}

fn name_of(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "style".into())
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

/// The view far out: the graph painted on a globe round its centre, the
/// view's own size, past the rim cleared. Each is a run of the graph, on
/// tiles a few levels deep.
fn globe(drawer: &Drawer, profile: &Profile, grammar: &Grammar, ask: &Ask) -> Result<Picture, String> {
    let view = &ask.view;
    let (lng, lat) = drawland_grid::scene::geographic(view.center);
    let mut grammar = grammar.clone();
    crate::globe::make(&mut grammar);
    grammar.output.screen = Some(view.screen);
    let scale = crate::globe::scale_of(view);
    grammar.output.scale.0 = scale;
    grammar.output.tile_zoom = Some(view.tile_zoom());
    grammar.output.fit_screen()?;
    Ok(drawer.paint_globe(&grammar, &Place::at(lat, lng), profile.dir(), &(), crate::globe::radius(scale), ask.names_past_rim)?.picture)
}

/// A request: its method, target (`/frame?…`) and body.
struct Request {
    post: bool,
    target: String,
    body: Vec<u8>,
}

/// One connection's requests, kept alive.
fn serve(stream: TcpStream, styles: &Styles, want: &mpsc::Sender<(usize, Ask)>, page: &str) {
    let Ok(mut out) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    while let Some(req) = request(&mut reader) {
        let (path, query) = req.target.split_once('?').unwrap_or((&req.target, ""));
        let q = Query(query);
        let failed = |e: String| Reply::status("422 Unprocessable Content", "text/plain; charset=utf-8", e.into_bytes());
        let reply = match path {
            "/" => Reply::ok("text/html; charset=utf-8", page.as_bytes().to_vec()),
            "/frame" => frame(styles, want, &q).unwrap_or_else(failed),
            "/chrome" => chrome(styles, &q).unwrap_or_else(failed),
            "/edit" => Reply::ok("text/html; charset=utf-8", EDIT.as_bytes().to_vec()),
            "/source" if req.post => save(styles, &q, &req.body).unwrap_or_else(failed),
            "/source" => source(styles, &q).unwrap_or_else(failed),
            "/reload" => reload(styles, &q).unwrap_or_else(failed),
            _ => Reply::status("404 Not Found", "text/plain", b"not here".to_vec()),
        };
        if reply.send(&mut out).is_err() {
            return;
        }
    }
}

/// A query string's values, percent-decoded.
struct Query<'a>(&'a str);

impl Query<'_> {
    fn get(&self, key: &str) -> Option<String> {
        self.0.split('&').find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('='))).map(decode)
    }

    fn number(&self, key: &str) -> Result<f64, String> {
        self.get(key).ok_or_else(|| format!("{key} is missing"))?.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{key} is not a number"))
    }

    /// The style asked for, by name; the first when none is.
    fn style<'s>(&self, styles: &'s Styles) -> Result<(usize, &'s Style), String> {
        match self.get("style") {
            None => Ok((0, &styles[0])),
            Some(name) => styles.iter().enumerate().find(|(_, s)| s.name == name).ok_or_else(|| format!("no style {name:?}")),
        }
    }
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The view the query asks for, painted from the scene there is; a new
/// scene asked for when this one will not hold it much longer.
fn frame(styles: &Styles, want: &mpsc::Sender<(usize, Ask)>, q: &Query) -> Result<Reply, String> {
    let (i, style) = q.style(styles)?;
    let mut inner = style.inner.lock().unwrap();
    let mut s = inner.screen;
    s.width = (q.number("w")? as u32).clamp(1, 4096);
    s.height = (q.number("h")? as u32).clamp(1, 4096);
    let view = View::at(q.number("lng")?, q.number("lat")?, crate::globe::clamp(q.number("scale")?, s.size()), s);
    if crate::globe::far_view(&view) {
        let ask = Ask { view, names_past_rim: q.get("rim").as_deref() == Some("over") };
        return globe_frame(style, i, want, ask, inner);
    }
    if !inner.scene.as_ref().is_some_and(|sc| sc.covers(&view)) && !inner.building {
        let _ = want.send((i, Ask { view, names_past_rim: false }));
        inner.building = true;
    }
    // The first scene is waited for; after that, the old one is painted.
    let deadline = Instant::now() + Duration::from_secs(120);
    while inner.scene.is_none() {
        if let Some(e) = inner.error.take() {
            return Err(e);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("no scene yet".into());
        }
        inner = style.ready.wait_timeout(inner, left).unwrap().0;
    }
    let scene = inner.scene.clone().unwrap();
    let building = inner.building;
    let who = q.get("who").unwrap_or_default();
    let mut recall = inner.recall.remove(&who).unwrap_or_default();
    drop(inner);
    // The time, for terra incognita's rings to run.
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let seconds = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    let (picture, (dx, dy)) = scene.render_recalling(&view, &mut recall, seconds);
    let mut inner = style.inner.lock().unwrap();
    // Pages come and go: a few dozen remembered.
    if inner.recall.len() >= 64 {
        inner.recall.clear();
    }
    inner.recall.insert(who, recall);
    let body = match &picture {
        Picture::Bitmap(b) => b.pbm(),
        Picture::Text(t) => text_runs(t).into_bytes(),
    };
    let kind = if matches!(picture, Picture::Bitmap(_)) { "image/x-portable-bitmap" } else { "application/json" };
    let mut reply = Reply::ok(kind, body);
    reply.headers = format!("X-Scene: {}\r\nX-Shift: {dx:.4},{dy:.4}\r\n", if building { "building" } else { "ready" });
    Ok(reply)
}

/// A frame far out: the globe painted for this view, or until it comes the
/// last one (`X-Scene: building`, and the page asks again).
fn globe_frame(style: &Style, i: usize, want: &mpsc::Sender<(usize, Ask)>, ask: Ask, mut inner: std::sync::MutexGuard<Inner>) -> Result<Reply, String> {
    let have = inner.globe.as_ref().is_some_and(|(a, _)| *a == ask);
    if !have && !inner.building {
        let _ = want.send((i, ask));
        inner.building = true;
    }
    // A globe takes a tenth of a second or so: the one for this view is
    // waited for a little, then the last one is sent while it comes.
    let start = Instant::now();
    let soon = start + Duration::from_secs(2);
    let deadline = start + Duration::from_secs(120);
    loop {
        if let Some(e) = inner.error.take() {
            return Err(e);
        }
        let exact = inner.globe.as_ref().is_some_and(|(a, _)| *a == ask);
        let now = Instant::now();
        if exact || (inner.globe.is_some() && now >= soon) {
            break;
        }
        if now >= deadline {
            return Err("no globe yet".into());
        }
        let until = if inner.globe.is_some() { soon } else { deadline };
        inner = style.ready.wait_timeout(inner, until.saturating_duration_since(now)).unwrap().0;
        if !inner.building && !inner.globe.as_ref().is_some_and(|(a, _)| *a == ask) {
            let _ = want.send((i, ask));
            inner.building = true;
        }
    }
    let (shown, picture) = inner.globe.clone().unwrap();
    let ready = shown == ask;
    drop(inner);
    let (body, kind) = match &*picture {
        Picture::Bitmap(b) => (b.pbm(), "image/x-portable-bitmap"),
        Picture::Text(t) => (text_runs(t).into_bytes(), "application/json"),
    };
    let mut reply = Reply::ok(kind, body);
    reply.headers = format!("X-Scene: {}\r\nX-Shift: 0,0\r\nX-Globe: 1\r\n", if ready { "ready" } else { "building" });
    Ok(reply)
}

/// The interface for the page's state (`open`, `column`, `theme`, `size`,
/// `fit`) and its map now (`lat`, `lng`, `scale`, `w`, `h`, `ms`,
/// `loading`, `error`), on a grid `w` × `h` units.
fn chrome(styles: &Styles, q: &Query) -> Result<Reply, String> {
    let (_, style) = q.style(styles)?;
    let screen = style.inner.lock().unwrap().screen;
    let kind = screen.kind;
    let mut ui = Ui::new(&style.name, kind);
    ui.open = q.get("open").filter(|o| !o.is_empty());
    ui.column = q.get("column").as_deref() == Some("1");
    if let Some(t) = q.get("theme") {
        ui.theme = t;
    }
    ui.size = q.get("size").and_then(|s| s.parse().ok());
    ui.fit_screen = q.get("fit").as_deref() == Some("screen");
    ui.names_past_rim = q.get("rim").as_deref() == Some("over");
    ui.scroll = q.number("scroll").unwrap_or(0.0).max(0.0) as usize;
    let (w, h) = ((q.number("w")? as usize).clamp(1, 4096), (q.number("h")? as usize).clamp(1, 4096));
    let error = q.get("error").filter(|e| !e.is_empty());
    let now = Now {
        lat: q.number("lat")?,
        lng: q.number("lng")?,
        scale: q.number("scale")?,
        kind,
        size: (w as u32, h as u32),
        screen: (screen.width, screen.height),
        ms: q.get("ms").and_then(|m| m.parse().ok()),
        loading: q.get("loading").as_deref() == Some("1"),
        error: error.as_deref(),
    };
    let list: Vec<chrome::Style> = styles.iter().map(|s| chrome::Style { name: s.name.clone(), path: s.path.clone(), kind }).collect();
    let model = chrome::model(&ui, &list, &now, Host::Page);
    let zones = |c: &Chrome| -> serde_json::Value { c.zones.iter().map(|z| serde_json::json!([z.x, z.y, z.w, z.h, z.action])).collect() };
    let body = match kind {
        ScreenKind::Text => {
            let laid = Chrome::lay(&model, w, h);
            let palette = Palette::named(&ui.theme);
            let rows: Vec<serde_json::Value> = laid
                .runs()
                .into_iter()
                .enumerate()
                .map(|(y, runs)| {
                    runs.into_iter()
                        .map(|(text, role)| match role {
                            None => serde_json::json!([text.chars().count()]),
                            Some(r) => {
                                let (fg, bg) = palette.at(r, y, laid.height);
                                serde_json::json!([text, fg.hex(), bg.hex()])
                            }
                        })
                        .collect()
                })
                .collect();
            serde_json::json!({ "cols": w, "rows": h, "cell": [1, 1], "free": laid.free, "scroll": laid.scroll, "runs": rows, "zones": zones(&laid) })
        }
        ScreenKind::Bitmap => {
            let font = bitmap_font(h);
            let (cols, rows) = ((w / font.width).max(1), (h / font.height).max(1));
            let laid = Chrome::lay(&model, cols, rows);
            let pixels = Pixels::new(&laid, font);
            // Two planes a bit a pixel, rows padded to a byte: where the
            // interface covers the map, and its ink there.
            let stride = w.div_ceil(8);
            let (mut cover, mut ink) = (vec![0u8; stride * h], vec![0u8; stride * h]);
            for y in 0..h {
                for x in 0..w {
                    if let Some(black) = pixels.at(x, y) {
                        cover[y * stride + x / 8] |= 0x80 >> (x % 8);
                        if black {
                            ink[y * stride + x / 8] |= 0x80 >> (x % 8);
                        }
                    }
                }
            }
            serde_json::json!({
                "cols": cols, "rows": rows, "cell": [font.width, font.height], "w": w, "h": h,
                // Pixels across left of the column.
                "free": if laid.free < cols { laid.free * font.width } else { w },
                "scroll": laid.scroll,
                "cover": base64(&cover), "ink": base64(&ink), "zones": zones(&laid),
            })
        }
    };
    Ok(Reply::ok("application/json", body.to_string().into_bytes()))
}

/// A style's script, for the editor.
fn source(styles: &Styles, q: &Query) -> Result<Reply, String> {
    let (_, style) = q.style(styles)?;
    let text = std::fs::read_to_string(&style.path).map_err(|e| format!("{}: {e}", style.path.display()))?;
    let mut reply = Reply::ok("text/plain; charset=utf-8", text.into_bytes());
    reply.headers = format!("X-File: {}\r\n", style.path.display());
    Ok(reply)
}

/// A style's script written from the editor, and read again; its error
/// (the file kept as written) when it does not run.
fn save(styles: &Styles, q: &Query, body: &[u8]) -> Result<Reply, String> {
    let (_, style) = q.style(styles)?;
    let text = std::str::from_utf8(body).map_err(|_| "the script is not UTF-8".to_string())?;
    std::fs::write(&style.path, text).map_err(|e| format!("{}: {e}", style.path.display()))?;
    reload(styles, q)
}

/// A style read again from its file: its scenes dropped, painted anew.
fn reload(styles: &Styles, q: &Query) -> Result<Reply, String> {
    let (_, style) = q.style(styles)?;
    let profile = Profile::read(&style.path)?;
    // The graph alone: the place comes from the page, and an address in the
    // script is not looked up again.
    let drawer = Drawer::new(crate::Settings { offline: true, ..crate::Settings::default() });
    let grammar = drawer.graph(&profile, &Place::at(0.0, 0.0), &())?;
    let screen = grammar.output.screen.ok_or("the style has no screen: output({ screen: … })")?;
    let mut inner = style.inner.lock().unwrap();
    if screen.kind != inner.screen.kind {
        return Err("the style's screen changed kind: serve it on a port of its own".into());
    }
    inner.profile = profile;
    inner.grammar = grammar;
    inner.screen = screen;
    inner.scene = None;
    inner.error = None;
    inner.building = false;
    inner.generation += 1;
    Ok(Reply::ok("text/plain; charset=utf-8", b"ok".to_vec()))
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

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The next request, its headers read past; `None` when the connection
/// closes.
fn request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut words = line.split_whitespace();
    let post = words.next()? == "POST";
    let target = words.next()?.to_string();
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
    let mut body = vec![0; length.min(8 << 20)];
    reader.read_exact(&mut body).ok()?;
    Some(Request { post, target, body })
}

struct Reply {
    status: &'static str,
    kind: &'static str,
    body: Vec<u8>,
    /// More header lines, each ending `\r\n`.
    headers: String,
}

impl Reply {
    fn ok(kind: &'static str, body: Vec<u8>) -> Reply {
        Reply::status("200 OK", kind, body)
    }

    fn status(status: &'static str, kind: &'static str, body: Vec<u8>) -> Reply {
        Reply { status, kind, body, headers: String::new() }
    }

    fn send(self, stream: &mut TcpStream) -> std::io::Result<()> {
        let head = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n{}\r\n",
            self.status,
            self.kind,
            self.body.len(),
            self.headers
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_percent_decoding() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(decode("a%20b+c%C3%A9%"), "a b cé%");
    }
}
