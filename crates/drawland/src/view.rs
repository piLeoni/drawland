//! `drawland view`: a screen profile's map in the terminal, the interface
//! round it drawn in the terminal's own characters ([`crate::chrome`]): a
//! bar on top with the styles and themes, a column that opens on the right,
//! a status line. Keys and the mouse move it: arrows (or h j k l) and a drag
//! move, + − and the wheel zoom, a click presses what is under it. `e`
//! opens the style in `$EDITOR` and draws it again when it is closed.
//!
//! The screen is the terminal's size (a bitmap shown in Braille, two pixels
//! across and four down a cell); every move paints it again, from the cache
//! once its tiles are read. A press on the interface alone lays it out again
//! over the last map, without painting.

use crate::chrome::{self, Host, Now, Style, Ui};
use crate::{Drawer, Place, Profile};
use drawland_core::frame::Frame;
use drawland_core::grammar::Grammar;
use drawland_core::paint::{Screen, ScreenKind, CELL_DOTS};
use drawland_grid::ui::{Chrome, Palette};
use drawland_grid::{Picture, Text};
use std::path::Path;
use std::time::Instant;

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

/// Across, how many of a screen's units a terminal cell holds.
fn units_a_cell(kind: ScreenKind) -> f64 {
    match kind {
        ScreenKind::Text => 1.0,
        ScreenKind::Bitmap => CELL_DOTS.0 as f64,
    }
}

/// The style drawn: its profile, graph and screen.
struct Drawn {
    profile: Profile,
    grammar: Grammar,
    screen: Screen,
}

impl Drawn {
    fn read(drawer: &Drawer, path: &Path, place: &Place) -> Result<(Drawn, (f64, f64)), String> {
        let profile = Profile::read(path)?;
        let (grammar, screen, at) = open(drawer, &profile, place, "view")?;
        Ok((Drawn { profile, grammar, screen }, at))
    }
}

pub fn run(drawer: &Drawer, path: &Path, place: &Place) -> Result<(), String> {
    let (mut drawn, (mut lng, mut lat)) = Drawn::read(drawer, path, place)?;
    let styles = chrome::styles(path, None);
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut ui = Ui::new(&name, drawn.screen.kind);
    // The style drawn now, by name: the one the menu chose once it is read.
    let mut current = name.clone();
    let mut term = terminal::Raw::enter()?;

    // The last map painted, and whether the next pass must paint again.
    let mut picture: Option<Picture> = None;
    let mut stale = true;
    // Where the names were, so they hold still as the map zooms.
    let mut recall = drawland_grid::Recall::default();
    let mut error: Option<String> = None;
    let mut ms = None;
    // Where a drag last was, in cells.
    let mut drag: Option<(i64, i64)> = None;
    loop {
        let (cols, rows) = terminal::size();
        let mut s = drawn.screen;
        (s.width, s.height) = match s.kind {
            ScreenKind::Text => (cols, rows),
            ScreenKind::Bitmap => (cols * CELL_DOTS.0, rows * CELL_DOTS.1),
        };
        drawn.grammar.output.screen = Some(s);
        drawn.grammar.output.fit_screen()?;
        let now = |ms, loading, error: Option<&str>| {
            let n = Now {
                lat,
                lng,
                scale: drawn.grammar.output.scale.0,
                kind: s.kind,
                size: (s.width, s.height),
                screen: (drawn.screen.width, drawn.screen.height),
                ms,
                loading,
                error,
            };
            chrome::model(&ui, &styles, &n, Host::Terminal)
        };
        let palette = Palette::named(&ui.theme);
        let show = |picture: &Option<Picture>, model: &drawland_grid::ui::Model| {
            let mut text = match picture {
                Some(Picture::Text(t)) => t.clone(),
                Some(Picture::Bitmap(b)) => b.braille(),
                None => Text::new(cols as usize, rows as usize),
            };
            let chrome = Chrome::lay(model, text.width, text.height);
            text.overlay(&chrome, &palette);
            (text.ansi(), chrome)
        };
        if stale {
            let (ansi, _) = show(&picture, &now(None, true, None));
            term.frame(&ansi);
            let t = Instant::now();
            // Far out, a globe round the centre (`crate::globe`).
            let far = crate::globe::far(drawn.grammar.output.scale.0, s.size());
            let mut grammar = drawn.grammar.clone();
            if far {
                crate::globe::make(&mut grammar);
            }
            match drawer.paint_recalling(&grammar, &Place::at(lat, lng), drawn.profile.dir(), &(), &mut recall) {
                Ok(p) => {
                    let mut p = p.picture;
                    if far {
                        p.globe(crate::globe::radius(grammar.output.scale.0));
                    }
                    picture = Some(p);
                    error = None;
                    ms = Some(t.elapsed().as_millis() as u64);
                }
                Err(e) => error = Some(e),
            }
            stale = false;
        }
        let (ansi, laid) = show(&picture, &now(ms, false, error.as_deref()));
        term.frame(&ansi);

        // What the keys and the mouse ask, gathered while the last paint was under way.
        let (mut across, mut down, mut zoom) = (0.0f64, 0.0f64, 0.0f64);
        let mut again = false;
        for event in term.events() {
            use terminal::Event::*;
            let mut action: Option<String> = None;
            match event {
                Quit => return Ok(()),
                Key('q') if ui.open.is_none() => return Ok(()),
                Escape if ui.open.is_some() => {
                    ui.open = None;
                }
                Escape => return Ok(()),
                Key('r') => again = true,
                Key('+' | '=') => zoom += 1.0,
                Key('-' | '_') => zoom -= 1.0,
                Key('e') => action = Some("edit".into()),
                Key(c) => action = ui.key(c),
                Arrow(dx, dy) => {
                    across += dx as f64 * cols as f64 / 4.0;
                    down += dy as f64 * rows as f64 / 4.0;
                }
                Press(x, y) => match laid.hit(x as usize, y as usize) {
                    Some(a) => action = Some(a.to_string()),
                    None if laid.cell(x as usize, y as usize).is_none() => {
                        ui.open = None;
                        drag = Some((x, y));
                    }
                    None => {}
                },
                Drag(x, y) => {
                    if let Some((x0, y0)) = drag {
                        across -= (x - x0) as f64;
                        down -= (y - y0) as f64;
                        drag = Some((x, y));
                    }
                }
                Release => drag = None,
                Wheel(n) => zoom -= n as f64 * 0.5,
            }
            let Some(action) = action else { continue };
            if ui.act(&action) {
                if ui.style != current {
                    if let Some(style) = styles.iter().find(|st| st.name == ui.style) {
                        switch(drawer, &mut drawn, style, (lng, lat), &mut error);
                        current = ui.style.clone();
                        again = true;
                    }
                }
                continue;
            }
            let (verb, arg) = action.split_once(':').unwrap_or((&action, ""));
            match verb {
                "zoom" => zoom += arg.parse::<f64>().unwrap_or(0.0),
                "reload" => {
                    reload(drawer, &mut drawn, &styles, &current, path, (lng, lat), &mut error);
                    again = true;
                }
                "edit" => {
                    let file = styles.iter().find(|st| st.name == ui.style).map(|st| st.path.clone()).unwrap_or_else(|| path.to_path_buf());
                    term.suspend();
                    let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".into());
                    let ran = std::process::Command::new("sh").arg("-c").arg(format!("{editor} \"$1\"")).arg("sh").arg(&file).status();
                    term = terminal::Raw::enter()?;
                    if let Err(e) = ran {
                        error = Some(format!("{editor}: {e}"));
                    }
                    reload(drawer, &mut drawn, &styles, &current, path, (lng, lat), &mut error);
                    again = true;
                }
                "link" => {
                    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                    let _ = std::process::Command::new(opener).arg(arg).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                }
                _ => {}
            }
        }

        if across != 0.0 || down != 0.0 {
            let frame = Frame::new(&drawn.grammar.output, (lng, lat));
            let (w, h) = (drawn.grammar.output.width.0, drawn.grammar.output.height.0);
            let (mx, my) = (w / cols as f64, h / rows as f64);
            if let Some((x, y)) = frame.to_world(w / 2.0 + across * mx, h / 2.0 + down * my) {
                (lng, lat) = (((x + 540.0) % 360.0) - 180.0, y.clamp(-85.0, 85.0));
            }
            stale = true;
        }
        if zoom != 0.0 {
            let s = drawn.grammar.output.scale.0 * 2f64.powf(-zoom);
            // From ten centimetres a unit to the whole globe on the screen.
            let units = (drawn.grammar.output.width.0, drawn.grammar.output.height.0);
            drawn.grammar.output.scale.0 = crate::globe::clamp(s, units);
            stale = true;
        }
        stale |= again;
    }
}

/// Draw `style` from now on, at the same place and the same ground a cell.
fn switch(drawer: &Drawer, drawn: &mut Drawn, style: &Style, (lng, lat): (f64, f64), error: &mut Option<String>) {
    match Drawn::read(drawer, &style.path, &Place::at(lat, lng)) {
        Ok((mut next, _)) => {
            let k = units_a_cell(drawn.screen.kind) / units_a_cell(next.screen.kind);
            next.grammar.output.scale.0 = drawn.grammar.output.scale.0 * k;
            *drawn = next;
            *error = None;
        }
        Err(e) => *error = Some(format!("{}: {e}", style.name)),
    }
}

/// The style drawn read again from its file (after an edit), the place and
/// scale kept.
fn reload(drawer: &Drawer, drawn: &mut Drawn, styles: &[Style], name: &str, first: &Path, (lng, lat): (f64, f64), error: &mut Option<String>) {
    let path = styles.iter().find(|s| s.name == name).map(|s| s.path.clone()).unwrap_or_else(|| first.to_path_buf());
    let scale = drawn.grammar.output.scale.0;
    match Drawn::read(drawer, &path, &Place::at(lat, lng)) {
        Ok((mut next, _)) => {
            next.grammar.output.scale.0 = scale;
            *drawn = next;
            *error = None;
        }
        Err(e) => *error = Some(e),
    }
}

#[cfg(unix)]
mod terminal {
    use std::io::{Read, Write};

    /// What came in from the keys and the mouse.
    #[derive(Debug, PartialEq)]
    pub enum Event {
        Key(char),
        /// An arrow: across and down, −1 to 1.
        Arrow(i32, i32),
        Escape,
        Quit,
        /// The left button pressed, dragged, let go, at a cell (0-based).
        Press(i64, i64),
        Drag(i64, i64),
        Release,
        /// The wheel: up 1, down −1.
        Wheel(i32),
    }

    /// The terminal in raw mode on its alternate screen, the mouse
    /// reported, given back as it was when dropped.
    pub struct Raw {
        saved: libc::termios,
        live: bool,
        /// stderr as it was: while the map is up it goes nowhere, or a
        /// line logged would scroll the screen.
        stderr: i32,
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
            // The alternate screen, no cursor, the mouse's presses, drags
            // and wheel in SGR's form.
            print!("\x1b[?1049h\x1b[?25l\x1b[2J\x1b[?1000h\x1b[?1002h\x1b[?1006h");
            let _ = std::io::stdout().flush();
            let stderr = unsafe { libc::dup(libc::STDERR_FILENO) };
            let null = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY) };
            if null >= 0 {
                unsafe { libc::dup2(null, libc::STDERR_FILENO) };
                unsafe { libc::close(null) };
            }
            Ok(Raw { saved, live: true, stderr })
        }

        /// The terminal given back for a while (to an editor).
        pub fn suspend(&mut self) {
            if self.live {
                leave(&self.saved, self.stderr);
                self.live = false;
            }
        }

        pub fn frame(&self, ansi: &str) {
            let mut out = String::from("\x1b[H");
            for (i, line) in ansi.lines().enumerate() {
                if i > 0 {
                    out.push_str("\r\n");
                }
                out.push_str(line);
                out.push_str("\x1b[0m");
            }
            print!("{out}");
            let _ = std::io::stdout().flush();
        }

        /// What came in: one thing waited for, then whatever else is already
        /// there, so a run of moves is painted once.
        pub fn events(&self) -> Vec<Event> {
            let mut events = Vec::new();
            let mut wait = -1;
            while let Some(bytes) = read(wait) {
                wait = 0;
                parse(&bytes, &mut events);
            }
            events
        }
    }

    /// Bytes from the terminal as events.
    pub fn parse(bytes: &[u8], events: &mut Vec<Event>) {
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == 0x1b && bytes.get(i + 1) == Some(&b'[') {
                // The mouse: ESC [ < button ; x ; y (M pressed | m let go).
                if bytes.get(i + 2) == Some(&b'<') {
                    let end = (i + 3..bytes.len()).find(|&j| bytes[j] == b'M' || bytes[j] == b'm');
                    if let Some(end) = end {
                        let fields: Vec<i64> = std::str::from_utf8(&bytes[i + 3..end]).unwrap_or("").split(';').filter_map(|f| f.parse().ok()).collect();
                        if let [button, x, y] = fields[..] {
                            let (x, y) = (x - 1, y - 1);
                            let event = match (button, bytes[end]) {
                                (64, _) => Some(Event::Wheel(1)),
                                (65, _) => Some(Event::Wheel(-1)),
                                (0, b'M') => Some(Event::Press(x, y)),
                                (32, b'M') => Some(Event::Drag(x, y)),
                                (0, b'm') => Some(Event::Release),
                                _ => None,
                            };
                            events.extend(event);
                        }
                        i = end + 1;
                        continue;
                    }
                }
                let arrow = match bytes.get(i + 2) {
                    Some(b'A') => Some((0, -1)),
                    Some(b'B') => Some((0, 1)),
                    Some(b'C') => Some((1, 0)),
                    Some(b'D') => Some((-1, 0)),
                    _ => None,
                };
                if let Some((dx, dy)) = arrow {
                    events.push(Event::Arrow(dx, dy));
                }
                i += 3;
                continue;
            }
            match b {
                0x1b => events.push(Event::Escape),
                3 => events.push(Event::Quit),
                b'k' => events.push(Event::Arrow(0, -1)),
                b'j' => events.push(Event::Arrow(0, 1)),
                b'l' => events.push(Event::Arrow(1, 0)),
                b'h' => events.push(Event::Arrow(-1, 0)),
                b if b.is_ascii_graphic() => events.push(Event::Key(b as char)),
                _ => {}
            }
            i += 1;
        }
    }

    /// What is waiting on stdin, after up to `wait` ms (`-1`: until a key).
    fn read(wait: i32) -> Option<Vec<u8>> {
        let mut fds = libc::pollfd { fd: libc::STDIN_FILENO, events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut fds, 1, wait) } <= 0 {
            return None;
        }
        let mut buf = [0u8; 256];
        let n = std::io::stdin().read(&mut buf).ok()?;
        (n > 0).then(|| buf[..n].to_vec())
    }

    fn leave(saved: &libc::termios, stderr: i32) {
        if stderr >= 0 {
            unsafe { libc::dup2(stderr, libc::STDERR_FILENO) };
            unsafe { libc::close(stderr) };
        }
        print!("\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = std::io::stdout().flush();
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, saved) };
    }

    impl Drop for Raw {
        fn drop(&mut self) {
            if self.live {
                leave(&self.saved, self.stderr);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keys_arrows_and_the_mouse_are_read() {
            let mut e = Vec::new();
            parse(b"\x1b[A+q\x1b[<0;5;3M\x1b[<32;7;3M\x1b[<0;7;3m\x1b[<65;1;1M", &mut e);
            assert_eq!(
                e,
                vec![Event::Arrow(0, -1), Event::Key('+'), Event::Key('q'), Event::Press(4, 2), Event::Drag(6, 2), Event::Release, Event::Wheel(-1)]
            );
        }
    }
}

#[cfg(not(unix))]
mod terminal {
    #[derive(Debug, PartialEq)]
    pub enum Event {
        Key(char),
        Arrow(i32, i32),
        Escape,
        Quit,
        Press(i64, i64),
        Drag(i64, i64),
        Release,
        Wheel(i32),
    }

    pub struct Raw;

    pub fn size() -> (u32, u32) {
        (80, 24)
    }

    impl Raw {
        pub fn enter() -> Result<Raw, String> {
            Err("drawland view runs on macOS and Linux terminals for now".into())
        }
        pub fn suspend(&mut self) {}
        pub fn frame(&self, _: &str) {}
        pub fn events(&self) -> Vec<Event> {
            vec![Event::Quit]
        }
    }
}
