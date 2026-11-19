//! `drawland serve`: a screen profile's map in a web page on this machine,
//! moved by dragging, the keys and the wheel. The page asks for one screen
//! at a time (`/paint?lat&lng&scale&w&h`): a bitmap comes back as a PNG,
//! text as the cells of a `<pre>`. One profile a port, so a bitmap and a
//! text site run side by side, each its own page.

use crate::view::open;
use crate::{Drawer, Place, Profile};
use drawland_core::paint::ScreenKind;
use drawland_grid::{Format, Picture};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

const PAGE: &str = include_str!("serve.html");

pub fn run(drawer: &Drawer, profile: &Profile, place: &Place, port: u16) -> Result<(), String> {
    let (grammar, screen, (lng, lat)) = open(drawer, profile, place, "serve")?;
    let title = grammar.title.clone().unwrap_or_else(|| "draw.land".into());
    let kind = match screen.kind {
        ScreenKind::Bitmap => "bitmap",
        ScreenKind::Text => "text",
    };
    let start = serde_json::json!({
        "title": title,
        "kind": kind,
        "lat": lat,
        "lng": lng,
        "scale": grammar.output.scale.0,
        "width": screen.width,
        "height": screen.height,
        "webmercator": grammar.output.projection == drawland_core::profile::ProjectionMode::Webmercator,
    });
    let page = PAGE.replace("/*START*/null", &start.to_string()).replace("<title>draw.land</title>", &format!("<title>{}</title>", escape(&title)));
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    eprintln!("{title}: http://localhost:{port}/  ({kind} {} × {}; Ctrl-C stops)", screen.width, screen.height);
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let Some(target) = request(&stream) else { continue };
        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        let reply = match path {
            "/" => Reply::ok("text/html; charset=utf-8", page.clone().into_bytes()),
            "/paint" => match paint(drawer, profile, &grammar, screen, query) {
                Ok(r) => r,
                Err(e) => Reply { status: "422 Unprocessable Content", kind: "text/plain; charset=utf-8", body: e.into_bytes() },
            },
            _ => Reply { status: "404 Not Found", kind: "text/plain", body: b"not here".to_vec() },
        };
        reply.send(&mut stream);
    }
    Ok(())
}

/// One screen at the query's place and scale, the query's size.
fn paint(drawer: &Drawer, profile: &Profile, grammar: &drawland_core::grammar::Grammar, screen: drawland_core::paint::Screen, query: &str) -> Result<Reply, String> {
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
    let (lat, lng) = (number("lat")?.clamp(-85.0, 85.0), number("lng")?);
    let mut grammar = grammar.clone();
    let mut s = screen;
    (s.width, s.height) = (number("w")? as u32, number("h")? as u32);
    grammar.output.screen = Some(s);
    grammar.output.scale.0 = number("scale")?.clamp(100.0, 100_000_000.0);
    grammar.output.fit_screen()?;
    let painting = drawer.paint_graph(&grammar, &Place::at(lat, lng), profile.dir(), &())?;
    Ok(match &painting.picture {
        Picture::Bitmap(_) => Reply::ok("image/png", painting.picture.encode(Format::Png, "")?),
        Picture::Text(t) => Reply::ok("text/html; charset=utf-8", t.html_rows().into_bytes()),
    })
}

/// The request's target (`/paint?…`), its headers read past; GET only.
fn request(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next()?, parts.next()?.to_string());
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
    }
    (method == "GET").then_some(target)
}

struct Reply {
    status: &'static str,
    kind: &'static str,
    body: Vec<u8>,
}

impl Reply {
    fn ok(kind: &'static str, body: Vec<u8>) -> Reply {
        Reply { status: "200 OK", kind, body }
    }

    fn send(self, stream: &mut TcpStream) {
        let head = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            self.status,
            self.kind,
            self.body.len()
        );
        let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(&self.body));
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
