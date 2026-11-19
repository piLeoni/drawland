//! `drawland view`: a screen profile's map in the terminal, moved round
//! with the keys. The screen is the terminal's size (a bitmap shown in
//! Braille, two pixels across and four down a cell); every move paints it
//! again, from the cache once its tiles are read.

use crate::{Drawer, Place, Profile};
use drawland_core::frame::Frame;
use drawland_core::grammar::Grammar;
use drawland_core::paint::{Screen, ScreenKind, CELL_DOTS};

/// What the keys ask, gathered while the last paint was under way.
#[derive(Default)]
struct Moves {
    across: f64,
    down: f64,
    zoom: i32,
    quit: bool,
    again: bool,
}

/// A screen profile's graph and screen, and the place it opens on
/// `(lng, lat)`: the command line's, else the profile's.
pub(crate) fn open(drawer: &Drawer, profile: &Profile, place: &Place, command: &str) -> Result<(Grammar, Screen, (f64, f64)), String> {
    let grammar = drawer.graph(profile, place, &())?;
    let Some(screen) = grammar.output.screen else {
        return Err(format!("drawland {command} shows a screen: give the profile's output one, output({{ screen: {{ kind: \"text\", width, height }} }})"));
    };
    let at = match (place.lat, place.lng, &place.address, grammar.output.lat, grammar.output.lng, &grammar.output.address) {
        (Some(lat), Some(lng), ..) => (lng, lat),
        (_, _, Some(a), ..) => {
            let (lat, lng) = drawer.geocode(a)?;
            (lng, lat)
        }
        (_, _, None, Some(lat), Some(lng), _) => (lng, lat),
        (_, _, None, _, _, Some(a)) => {
            let (lat, lng) = drawer.geocode(a)?;
            (lng, lat)
        }
        _ => return Err("no place: give --address or --lat and --lng".into()),
    };
    Ok((grammar, screen, at))
}

pub fn run(drawer: &Drawer, profile: &Profile, place: &Place) -> Result<(), String> {
    let (mut grammar, screen, (mut lng, mut lat)) = open(drawer, profile, place, "view")?;
    let term = terminal::Raw::enter()?;
    loop {
        let (cols, rows) = terminal::size();
        let rows = rows.saturating_sub(1).max(1);
        let mut s = screen;
        (s.width, s.height) = match s.kind {
            ScreenKind::Text => (cols, rows),
            ScreenKind::Bitmap => (cols * CELL_DOTS.0, rows * CELL_DOTS.1),
        };
        grammar.output.screen = Some(s);
        grammar.output.fit_screen()?;
        let scale = ground(grammar.output.scale.0, s.kind);
        term.status(&format!("painting {lat:.5}, {lng:.5} · {scale} …"), rows);
        let status = match drawer.paint_graph(&grammar, &Place::at(lat, lng), profile.dir(), &()) {
            Ok(p) => {
                term.frame(&p.picture.ansi());
                format!(" {lat:.5}, {lng:.5} · {scale} · arrows move · + − zoom · q quits")
            }
            Err(e) => format!(" {e}"),
        };
        term.status(&status, rows);
        let moves = term.keys();
        if moves.quit {
            return Ok(());
        }
        if moves.again {
            continue;
        }
        let frame = Frame::new(&grammar.output, (lng, lat));
        let (w, h) = (grammar.output.width.0, grammar.output.height.0);
        if moves.across != 0.0 || moves.down != 0.0 {
            if let Some((x, y)) = frame.to_world(w / 2.0 + moves.across * w / 4.0, h / 2.0 + moves.down * h / 4.0) {
                (lng, lat) = (x, y.clamp(-85.0, 85.0));
            }
        }
        if moves.zoom != 0 {
            let s = grammar.output.scale.0 * 2f64.powi(-moves.zoom);
            // From ten centimetres a unit to a hundred kilometres.
            grammar.output.scale.0 = s.clamp(100.0, 100_000_000.0);
        }
    }
}

/// The ground a pixel or a character spans, from the scale (mm a unit).
fn ground(scale: f64, kind: ScreenKind) -> String {
    let unit = match kind {
        ScreenKind::Bitmap => "pixel",
        ScreenKind::Text => "character",
    };
    match scale / 1000.0 {
        m if m >= 1000.0 => format!("{:.1} km a {unit}", m / 1000.0),
        m if m >= 10.0 => format!("{m:.0} m a {unit}"),
        m => format!("{m:.1} m a {unit}"),
    }
}

#[cfg(unix)]
mod terminal {
    use super::Moves;
    use std::io::{Read, Write};

    /// The terminal in raw mode on its alternate screen, given back as it
    /// was when dropped.
    pub struct Raw {
        saved: libc::termios,
    }

    pub fn size() -> (u32, u32) {
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
        if ok && ws.ws_col > 0 && ws.ws_row > 0 {
            (ws.ws_col as u32, ws.ws_row as u32)
        } else {
            (80, 24)
        }
    }

    impl Raw {
        pub fn enter() -> Result<Raw, String> {
            let mut saved: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut saved) } != 0 {
                return Err("drawland view needs a terminal".into());
            }
            let mut raw = saved;
            unsafe { libc::cfmakeraw(&mut raw) };
            raw.c_cc[libc::VMIN] = 1;
            raw.c_cc[libc::VTIME] = 0;
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) };
            print!("\x1b[?1049h\x1b[?25l\x1b[2J");
            let _ = std::io::stdout().flush();
            Ok(Raw { saved })
        }

        pub fn frame(&self, ansi: &str) {
            let mut out = String::from("\x1b[H");
            for line in ansi.lines() {
                out.push_str(line);
                out.push_str("\x1b[0m\x1b[K\r\n");
            }
            print!("{out}");
            let _ = std::io::stdout().flush();
        }

        pub fn status(&self, text: &str, row: u32) {
            let (cols, _) = size();
            let shown: String = text.chars().take(cols as usize).collect();
            print!("\x1b[{};1H\x1b[0m\x1b[7m{shown}\x1b[K\x1b[0m", row + 1);
            let _ = std::io::stdout().flush();
        }

        /// The keys pressed: one waited for, then whatever else is already
        /// there, so a run of moves is painted once.
        pub fn keys(&self) -> Moves {
            let mut moves = Moves::default();
            let mut wait = -1;
            loop {
                let Some(bytes) = read(wait) else { break };
                wait = 0;
                let mut i = 0;
                while i < bytes.len() {
                    let b = bytes[i];
                    let arrow = (b == 0x1b && bytes.get(i + 1) == Some(&b'[')).then(|| bytes.get(i + 2).copied()).flatten();
                    match (arrow, b) {
                        (Some(b'A'), _) => moves.down -= 1.0,
                        (Some(b'B'), _) => moves.down += 1.0,
                        (Some(b'C'), _) => moves.across += 1.0,
                        (Some(b'D'), _) => moves.across -= 1.0,
                        (None, b'k' | b'w') => moves.down -= 1.0,
                        (None, b'j' | b's') => moves.down += 1.0,
                        (None, b'l' | b'd') => moves.across += 1.0,
                        (None, b'h' | b'a') => moves.across -= 1.0,
                        (None, b'+' | b'=') => moves.zoom += 1,
                        (None, b'-' | b'_') => moves.zoom -= 1,
                        (None, b'r') => moves.again = true,
                        (None, b'q' | 3) => moves.quit = true,
                        (None, 0x1b) if bytes.len() == 1 => moves.quit = true,
                        _ => {}
                    }
                    i += if arrow.is_some() { 3 } else { 1 };
                }
            }
            moves
        }
    }

    /// What is waiting on stdin, after up to `wait` ms (`-1`: until a key).
    fn read(wait: i32) -> Option<Vec<u8>> {
        let mut fds = libc::pollfd { fd: libc::STDIN_FILENO, events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut fds, 1, wait) } <= 0 {
            return None;
        }
        let mut buf = [0u8; 64];
        let n = std::io::stdin().read(&mut buf).ok()?;
        (n > 0).then(|| buf[..n].to_vec())
    }

    impl Drop for Raw {
        fn drop(&mut self) {
            print!("\x1b[0m\x1b[?25h\x1b[?1049l");
            let _ = std::io::stdout().flush();
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved) };
        }
    }
}

#[cfg(not(unix))]
mod terminal {
    use super::Moves;

    pub struct Raw;

    pub fn size() -> (u32, u32) {
        (80, 24)
    }

    impl Raw {
        pub fn enter() -> Result<Raw, String> {
            Err("drawland view runs on macOS and Linux terminals for now".into())
        }
        pub fn frame(&self, _: &str) {}
        pub fn status(&self, _: &str, _: u32) {}
        pub fn keys(&self) -> Moves {
            Moves { quit: true, ..Moves::default() }
        }
    }
}
