//! The interface drawn in the screen's own medium: character cells on a
//! text screen, Spleen glyphs on 1-bit pixels. One cell grid, laid out once
//! ([`Chrome::lay`]), then put over a text screen ([`Chrome::text`]) or
//! drawn in pixels ([`Chrome::bitmap`]); cells it leaves empty show the map.
//!
//! Its parts are a web page's: a bar on top (links on the left, menus that
//! drop down, links on the right), a column on the right that opens and
//! closes, a few tools floating top right while it is closed, and a status
//! line at the bottom. Each thing that can be pressed has a [`Zone`] naming
//! its action; the host turns presses and keys into actions and lays the
//! chrome out again.

use crate::font::Font;
use drawland_core::paint::Rgb;

/// What a cell is part of, which sets its colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The top bar and the status line.
    Bar,
    /// A thing to press, at rest.
    Button,
    /// The page shown, the option chosen: drawn inverse.
    On,
    /// A menu dropped down, the column, the floating tools.
    Panel,
    /// Quieter words: headings, hints.
    Dim,
    /// Borders and dividers.
    Rule,
}

/// A thing that can be pressed: a link, a menu's option, a tool.
#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: String,
    pub action: String,
    /// The page shown, or the option chosen.
    pub on: bool,
}

impl Button {
    pub fn new(label: impl Into<String>, action: impl Into<String>) -> Button {
        Button { label: label.into(), action: action.into(), on: false }
    }

    pub fn on(mut self, on: bool) -> Button {
        self.on = on;
        self
    }
}

/// A menu on the bar: its label (with `▾`) and the options it drops down.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub id: String,
    pub label: String,
    /// The label when the bar is short of room.
    pub short: String,
    pub options: Vec<Button>,
    /// On the bar's right, with its links there.
    pub right: bool,
}

/// A line of the column.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// A section's title.
    Head(String),
    Text(String),
    /// Words in the quieter colour.
    Dim(String),
    /// Buttons side by side.
    Row(Vec<Button>),
    Blank,
}

/// The interface to draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    /// The bar's left: the site's own links.
    pub links: Vec<Button>,
    pub menus: Vec<Menu>,
    /// The bar's right, after its menus.
    pub right: Vec<Button>,
    /// The menu dropped down, by id.
    pub open: Option<String>,
    /// The column's lines, when it is open.
    pub column: Option<Vec<Line>>,
    /// Floating top right while the column is closed.
    pub tools: Vec<Button>,
    /// The status line: what is shown, then a hint at its right.
    pub status: String,
    pub hint: String,
}

/// A place to press: cells `x..x + w`, `y..y + h`.
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub action: String,
}

/// The interface laid out on a grid of cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Chrome {
    pub width: usize,
    pub height: usize,
    cells: Vec<Option<(char, Role)>>,
    /// Later zones lie over earlier ones: a menu over the bar under it.
    pub zones: Vec<Zone>,
}

/// The column's width in cells on a screen `width` across.
fn column_width(width: usize) -> usize {
    (width / 3).clamp(22, 34).min(width)
}

impl Chrome {
    /// `model` laid out on `width` × `height` cells.
    pub fn lay(model: &Model, width: usize, height: usize) -> Chrome {
        let mut c = Chrome { width, height, cells: vec![None; width * height], zones: Vec::new() };
        if width == 0 || height == 0 {
            return c;
        }
        c.bar(model);
        if height >= 3 {
            c.status(model);
        }
        match &model.column {
            Some(lines) if height >= 4 => c.column(lines),
            _ if height >= 5 => c.tools(&model.tools),
            _ => {}
        }
        if let Some(open) = &model.open {
            if let Some(m) = model.menus.iter().find(|m| &m.id == open) {
                c.menu(m);
            }
        }
        c
    }

    /// The cell at `(x, y)`: its character and role, or `None` where the
    /// map shows.
    pub fn cell(&self, x: usize, y: usize) -> Option<(char, Role)> {
        self.cells[y * self.width + x]
    }

    /// The action pressed at cell `(x, y)`, the topmost zone's.
    pub fn hit(&self, x: usize, y: usize) -> Option<&str> {
        self.zones.iter().rev().find(|z| (z.x..z.x + z.w).contains(&x) && (z.y..z.y + z.h).contains(&y)).map(|z| z.action.as_str())
    }

    fn put(&mut self, x: usize, y: usize, ch: char, role: Role) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = Some((ch, role));
        }
    }

    /// `text` from `(x, y)`, no further than `end`; the cells written.
    fn write(&mut self, x: usize, y: usize, text: &str, role: Role, end: usize) -> usize {
        let mut n = 0;
        for ch in text.chars() {
            if x + n >= end.min(self.width) {
                break;
            }
            self.put(x + n, y, ch, role);
            n += 1;
        }
        n
    }

    fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, role: Role) {
        for yy in y..(y + h).min(self.height) {
            for xx in x..(x + w).min(self.width) {
                self.put(xx, yy, ' ', role);
            }
        }
    }

    fn zone(&mut self, x: usize, y: usize, w: usize, h: usize, action: &str) {
        if w > 0 && h > 0 {
            self.zones.push(Zone { x, y, w, h, action: action.to_string() });
        }
    }

    /// A button as ` label `, inverse when on; the cells it took.
    fn button(&mut self, x: usize, y: usize, b: &Button, rest: Role, end: usize) -> usize {
        let role = if b.on { Role::On } else { rest };
        let n = self.write(x, y, &format!(" {} ", b.label), role, end);
        self.zone(x, y, n, 1, &b.action);
        n
    }

    fn bar(&mut self, model: &Model) {
        let w = self.width;
        self.fill(0, 0, w, 1, Role::Bar);
        let width = |s: &str| s.chars().count() + 2;
        let links: usize = model.links.iter().map(|b| width(&b.label) + 1).sum();
        let right: usize = model.right.iter().map(|b| width(&b.label) + 1).sum();
        let menus = |short: bool| -> usize {
            model.menus.iter().map(|m| width(if short { &m.short } else { &m.label }) + 1).sum::<usize>() + 2
        };
        let short = 1 + links + menus(false) + right > w;
        let mut x = 1;
        for b in &model.links {
            x += self.button(x, 0, b, Role::Bar, w) + 1;
        }
        let (left, right_menus): (Vec<&Menu>, Vec<&Menu>) = model.menus.iter().partition(|m| !m.right);
        if !left.is_empty() {
            x += self.write(x, 0, "│", Role::Rule, w) + 1;
        }
        for m in left {
            x += self.menu_label(x, m, short, model.open.as_deref() == Some(&m.id), w) + 1;
        }
        // The right, from the edge in.
        let mut rx = w.saturating_sub(1);
        for b in model.right.iter().rev() {
            let n = width(&b.label);
            if rx < x + n {
                break;
            }
            rx -= n;
            self.button(rx, 0, b, Role::Bar, w);
            rx = rx.saturating_sub(1);
        }
        for m in right_menus.iter().rev() {
            let n = width(if short { &m.short } else { &m.label });
            if rx < x + n {
                break;
            }
            rx -= n;
            self.menu_label(rx, m, short, model.open.as_deref() == Some(&m.id), w);
            rx = rx.saturating_sub(1);
        }
    }

    fn menu_label(&mut self, x: usize, m: &Menu, short: bool, open: bool, end: usize) -> usize {
        let b = Button::new(if short { &m.short } else { &m.label }, format!("open:{}", m.id)).on(open);
        self.button(x, 0, &b, Role::Bar, end)
    }

    /// A menu dropped down under its label.
    fn menu(&mut self, m: &Menu) {
        let at = self.zones.iter().find(|z| z.y == 0 && z.action == format!("open:{}", m.id)).map(|z| z.x);
        let Some(at) = at else { return };
        let inner = m.options.iter().map(|o| o.label.chars().count() + 4).max().unwrap_or(4);
        let w = (inner + 2).min(self.width);
        let x = if m.right { (at + 1 + m.label.chars().count() + 2).saturating_sub(w).min(self.width - w) } else { at.min(self.width - w) };
        let h = (m.options.len() + 2).min(self.height.saturating_sub(1));
        if h < 3 {
            return;
        }
        self.border(x, 1, w, h);
        // A press anywhere on the menu is the menu's, not the map's.
        self.zone(x, 1, w, h, "none");
        for (i, o) in m.options.iter().enumerate().take(h - 2) {
            let y = 2 + i;
            let role = if o.on { Role::On } else { Role::Panel };
            self.fill(x + 1, y, w - 2, 1, role);
            self.write(x + 1, y, if o.on { " • " } else { "   " }, role, x + w - 1);
            self.write(x + 4, y, &o.label, role, x + w - 1);
            self.zone(x + 1, y, w - 2, 1, &o.action);
        }
    }

    /// A box: its border, its inside cleared.
    fn border(&mut self, x: usize, y: usize, w: usize, h: usize) {
        self.fill(x, y, w, h, Role::Panel);
        for xx in x + 1..x + w - 1 {
            self.put(xx, y, '─', Role::Rule);
            self.put(xx, y + h - 1, '─', Role::Rule);
        }
        for yy in y + 1..y + h - 1 {
            self.put(x, yy, '│', Role::Rule);
            self.put(x + w - 1, yy, '│', Role::Rule);
        }
        self.put(x, y, '┌', Role::Rule);
        self.put(x + w - 1, y, '┐', Role::Rule);
        self.put(x, y + h - 1, '└', Role::Rule);
        self.put(x + w - 1, y + h - 1, '┘', Role::Rule);
    }

    fn status(&mut self, model: &Model) {
        let (w, y) = (self.width, self.height - 1);
        self.fill(0, y, w, 1, Role::Bar);
        // The hint only where the status has its room beside it.
        let hint = model.hint.chars().count();
        let fits = model.status.chars().count() + hint + 5 <= w;
        let room = if fits { w - hint - 2 } else { w };
        self.write(1, y, &model.status, Role::Bar, room);
        if fits {
            self.write(w - hint - 1, y, &model.hint, Role::Dim, w);
        }
        self.zone(0, y, w, 1, "none");
    }

    fn column(&mut self, lines: &[Line]) {
        let cw = column_width(self.width);
        let x0 = self.width - cw;
        let (top, bottom) = (1, self.height - 1);
        self.fill(x0, top, cw, bottom - top, Role::Panel);
        for y in top..bottom {
            self.put(x0, y, '│', Role::Rule);
        }
        self.zone(x0, top, cw, bottom - top, "none");
        let (left, end) = (x0 + 2, self.width - 1);
        let mut y = top + 1;
        for line in lines {
            if y >= bottom {
                break;
            }
            match line {
                Line::Head(t) => {
                    self.write(left, y, &t.to_uppercase(), Role::Dim, end);
                }
                Line::Text(t) => {
                    self.write(left, y, t, Role::Panel, end);
                }
                Line::Dim(t) => {
                    self.write(left, y, t, Role::Dim, end);
                }
                Line::Row(buttons) => {
                    let mut x = left;
                    for b in buttons {
                        let n = b.label.chars().count() + 2;
                        if x + n > end && x > left {
                            y += 1;
                            x = left;
                            if y >= bottom {
                                break;
                            }
                        }
                        x += self.button(x, y, b, Role::Button, end) + 1;
                    }
                }
                Line::Blank => {}
            }
            y += 1;
        }
    }

    /// The tools, in a box top right, while the column is closed.
    fn tools(&mut self, tools: &[Button]) {
        if tools.is_empty() {
            return;
        }
        let inner: usize = tools.iter().map(|b| b.label.chars().count() + 2).sum::<usize>() + tools.len() - 1;
        let w = inner + 2;
        if w + 2 > self.width {
            return;
        }
        let x0 = self.width - w - 1;
        self.border(x0, 1, w, 3);
        self.zone(x0, 1, w, 3, "none");
        let mut x = x0 + 1;
        for (i, b) in tools.iter().enumerate() {
            if i > 0 {
                self.put(x, 2, '│', Role::Rule);
                x += 1;
            }
            x += self.button(x, 2, b, Role::Panel, x0 + w - 1);
        }
    }

    /// The chrome as rows of runs of one role, for a page to draw:
    /// `[text, role]`, or `[n]` for `n` cells the map shows through.
    pub fn runs(&self) -> Vec<Vec<(String, Option<Role>)>> {
        (0..self.height)
            .map(|y| {
                let mut runs: Vec<(String, Option<Role>)> = Vec::new();
                for x in 0..self.width {
                    let (ch, role) = match self.cell(x, y) {
                        Some((c, r)) => (c, Some(r)),
                        None => (' ', None),
                    };
                    match runs.last_mut() {
                        Some((t, r)) if *r == role => t.push(ch),
                        _ => runs.push((ch.to_string(), role)),
                    }
                }
                runs
            })
            .collect()
    }
}

/// A chrome's colours on a text screen: a theme's paper and ink, the rest
/// mixed from them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub paper: Rgb,
    pub ink: Rgb,
}

/// The themes of draw.land's page, by name: `paper`, `ink`, `silk`,
/// `abyss`, `ember`, `phosphor`.
pub const THEMES: &[(&str, Palette)] = &[
    ("paper", Palette { paper: Rgb(0xff, 0xff, 0xff), ink: Rgb(0x00, 0x00, 0x00) }),
    ("ink", Palette { paper: Rgb(0x0a, 0x0b, 0x0d), ink: Rgb(0xee, 0xf1, 0xf6) }),
    ("silk", Palette { paper: Rgb(0xf7, 0xf6, 0xf2), ink: Rgb(0x5f, 0x5d, 0x58) }),
    ("abyss", Palette { paper: Rgb(0x0b, 0x23, 0x26), ink: Rgb(0xe9, 0xdf, 0xc8) }),
    ("ember", Palette { paper: Rgb(0x12, 0x0e, 0x06), ink: Rgb(0xff, 0xbf, 0x3a) }),
    ("phosphor", Palette { paper: Rgb(0x0a, 0x0f, 0x0a), ink: Rgb(0x41, 0xff, 0x41) }),
];

impl Palette {
    /// A theme by name, else `ink`.
    pub fn named(name: &str) -> Palette {
        THEMES.iter().find(|(n, _)| *n == name).map(|(_, p)| *p).unwrap_or(THEMES[1].1)
    }

    fn mix(&self, t: f64) -> Rgb {
        let m = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        Rgb(m(self.paper.0, self.ink.0), m(self.paper.1, self.ink.1), m(self.paper.2, self.ink.2))
    }

    /// A role's ink and paper.
    pub fn colours(&self, role: Role) -> (Rgb, Rgb) {
        match role {
            Role::Bar => (self.ink, self.mix(0.07)),
            Role::Button => (self.ink, self.mix(0.14)),
            Role::On => (self.paper, self.ink),
            Role::Panel => (self.ink, self.paper),
            Role::Dim => (self.mix(0.55), self.paper),
            Role::Rule => (self.mix(0.3), self.paper),
        }
    }

    /// The colours of a role's cell where it sits: dim words and rules on
    /// the bar keep the bar's paper.
    pub fn at(&self, role: Role, row: usize, height: usize) -> (Rgb, Rgb) {
        let (fg, bg) = self.colours(role);
        let on_bar = row == 0 || row + 1 == height;
        match role {
            Role::Dim | Role::Rule if on_bar => (fg, self.colours(Role::Bar).1),
            _ => (fg, bg),
        }
    }
}

/// The pixel font a 1-bit chrome is written in, for a screen `height`
/// pixels tall: Spleen 6×12, or 5×8 on a small one.
pub fn bitmap_font(height: usize) -> &'static Font {
    let name = if height < 200 { "spleen-5x8" } else { "spleen-6x12" };
    crate::font::font(name).expect("built-in font")
}

/// One pixel of the chrome on a 1-bit screen `w` × `h`: `Some(true)` ink,
/// `Some(false)` paper, `None` the map. Cells are `font`'s size; the last
/// row and column stretch to the screen's edge.
pub struct Pixels<'a> {
    chrome: &'a Chrome,
    font: &'static Font,
}

impl<'a> Pixels<'a> {
    pub fn new(chrome: &'a Chrome, font: &'static Font) -> Pixels<'a> {
        Pixels { chrome, font }
    }

    pub fn at(&self, px: usize, py: usize) -> Option<bool> {
        let (fw, fh) = (self.font.width, self.font.height);
        let c = self.chrome;
        let (cx, cy) = ((px / fw).min(c.width.saturating_sub(1)), (py / fh).min(c.height.saturating_sub(1)));
        let (ch, role) = c.cell(cx, cy)?;
        let (x, y) = (px - cx * fw, py - cy * fh);
        let ink = glyph(self.font, ch, x, y);
        // The bar's rule under it and the status line's over it.
        let edge = (cy == 0 && py == fh - 1 && c.height > 1) || (cy + 1 == c.height && cy > 0 && py == cy * fh);
        Some(match role {
            Role::On => !ink,
            // A button at rest is underlined in dots. One bit has no grey:
            // quieter words are inked like the rest (a dither of a 6 × 12
            // glyph cannot be read).
            Role::Button => ink || (y + 1 == fh && px % 2 == 0) || edge,
            _ => ink || edge,
        })
    }
}

/// A glyph's pixel: the font's, or one drawn here for the few signs the
/// Spleen sizes lack (`▾ ↗ ≡ •`).
fn glyph(font: &Font, ch: char, x: usize, y: usize) -> bool {
    let (w, h) = (font.width as f64, font.height as f64);
    let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
    match ch {
        '▾' if !font.has(ch) => {
            // A triangle pointing down, a third of the cell tall.
            let top = (h * 0.42).floor();
            let rows = (w / 2.0).floor().max(2.0);
            let r = fy.floor() - top;
            r >= 0.0 && r < rows && (fx - w / 2.0).abs() <= (rows - r) - 0.5
        }
        '≡' if !font.has(ch) => {
            let row = |t: f64| (fy.floor() - (h * t).floor()).abs() < 0.5;
            (row(0.3) || row(0.5) || row(0.7)) && fx > w * 0.1 && fx < w * 0.9
        }
        '•' if !font.has(ch) => (fx - w / 2.0).hypot((fy - h * 0.55) * 1.0) <= w * 0.28,
        '↗' if !font.has(ch) => {
            // A diagonal from the bottom left, a head at the top right.
            let (x0, y0, x1, y1) = (w * 0.15, h * 0.75, w * 0.85, h * 0.3);
            let t = ((fx - x0) * (x1 - x0) + (fy - y0) * (y1 - y0)) / ((x1 - x0).powi(2) + (y1 - y0).powi(2));
            let (px, py) = (x0 + t * (x1 - x0), y0 + t * (y1 - y0));
            let shaft = (0.0..=1.0).contains(&t) && (fx - px).hypot(fy - py) < 0.75;
            let head_top = (fy - y1).abs() < 0.5 && fx >= w * 0.45 && fx <= x1 + 0.5;
            let head_side = (fx - x1).abs() < 0.5 && fy >= y1 && fy <= h * 0.6;
            shaft || head_top || head_side
        }
        _ => font.ink(ch, x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model {
            links: vec![Button::new("map", "link:/").on(true), Button::new("docs", "link:/docs")],
            menus: vec![
                Menu {
                    id: "style".into(),
                    label: "terminal ▾".into(),
                    short: "terminal▾".into(),
                    options: vec![Button::new("terminal", "style:terminal").on(true), Button::new("1-bit", "style:1-bit")],
                    right: false,
                },
                Menu { id: "theme".into(), label: "ink ▾".into(), short: "▾".into(), options: vec![Button::new("paper", "theme:paper")], right: true },
            ],
            right: vec![Button::new("GitHub ↗", "link:https://github.com/piLeoni/drawland")],
            open: None,
            column: None,
            tools: vec![Button::new("+", "zoom:1"), Button::new("−", "zoom:-1"), Button::new("≡", "column")],
            status: "45.43750, 12.33580 · 8 m a character".into(),
            hint: "s style · p panel".into(),
        }
    }

    /// The cell where `word` starts on a row.
    fn col(c: &Chrome, y: usize, word: &str) -> usize {
        let r = row(c, y);
        r[..r.find(word).unwrap()].chars().count()
    }

    fn row(c: &Chrome, y: usize) -> String {
        (0..c.width).map(|x| c.cell(x, y).map(|(ch, _)| ch).unwrap_or('.')).collect()
    }

    #[test]
    fn the_bar_holds_links_left_menus_and_github_right() {
        let c = Chrome::lay(&model(), 80, 24);
        let bar = row(&c, 0);
        assert!(bar.starts_with("  map   docs  │  terminal ▾ "), "{bar}");
        assert!(bar.trim_end().ends_with("ink ▾   GitHub ↗"), "{bar}");
        assert_eq!(c.cell(1, 0), Some((' ', Role::On)), "the page shown is inverse");
        assert_eq!(c.hit(col(&c, 0, "docs"), 0), Some("link:/docs"));
        assert!(row(&c, 23).contains("8 m a character") && row(&c, 23).ends_with("s style · p panel "));
        // The map shows everywhere else but the tools.
        assert_eq!(c.cell(10, 10), None);
        assert_eq!(c.hit(78 - 2, 2), Some("column"), "{}", row(&c, 2));
    }

    #[test]
    fn a_menu_drops_down_under_its_label_and_its_options_are_pressed() {
        let mut m = model();
        m.open = Some("style".into());
        let c = Chrome::lay(&m, 80, 24);
        let at = col(&c, 0, "terminal") - 1;
        assert_eq!(c.cell(at, 1), Some(('┌', Role::Rule)), "{}", row(&c, 1));
        assert!(row(&c, 2).contains(" • terminal"), "{}", row(&c, 2));
        assert_eq!(c.hit(at + 3, 3), Some("style:1-bit"));
        assert_eq!(c.hit(at + 3, 2), Some("style:terminal"));
    }

    #[test]
    fn the_column_takes_the_right_and_the_tools_go() {
        let mut m = model();
        m.column = Some(vec![Line::Head("screen".into()), Line::Row(vec![Button::new("10", "size:10"), Button::new("12", "size:12").on(true)])]);
        let c = Chrome::lay(&m, 90, 30);
        let x0 = 90 - column_width(90);
        assert_eq!(c.cell(x0, 5), Some(('│', Role::Rule)));
        assert!(row(&c, 2).contains("SCREEN"), "{}", row(&c, 2));
        assert!(row(&c, 3).contains(" 10   12 "), "{}", row(&c, 3));
        assert!(c.zones.iter().all(|z| z.action != "column"), "no floating tools");
        assert_eq!(c.hit(x0 + 3, 3), Some("size:10"));
    }

    #[test]
    fn a_narrow_bar_uses_the_short_labels() {
        let c = Chrome::lay(&model(), 34, 10);
        let bar = row(&c, 0);
        assert!(bar.contains("terminal▾"), "{bar}");
    }

    #[test]
    fn on_pixels_the_bar_is_ruled_and_the_map_shows_below_it() {
        let c = Chrome::lay(&model(), 50, 20);
        let font = crate::font::font("spleen-6x12").unwrap();
        let p = Pixels::new(&c, font);
        assert_eq!(p.at(100, 11), Some(true), "the rule under the bar");
        assert_eq!(p.at(100, 5 * 12), None, "the map");
        // The page shown is inverse: its padding is ink.
        assert_eq!(p.at(6, 5), Some(true));
    }
}
