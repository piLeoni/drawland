//! The interface drawn in the screen's own medium: character cells on a
//! text screen, Spleen glyphs on 1-bit pixels. One cell grid, laid out once
//! ([`Chrome::lay`]), then put over a text screen ([`Chrome::text`]) or
//! drawn in pixels ([`Chrome::bitmap`]); cells it leaves empty show the map.
//!
//! Its parts are a web page's: a bar on top (the site's name, links, menus
//! that drop down, links on the right), a column on the right that opens
//! and closes, a few tools floating top right while it is closed, and a
//! status line at the bottom.
//!
//! The look, both ways: a key's letter in the accent (underlined on 1 bit),
//! labels quieter than the values they name, sections titled over a rule,
//! a row of choices one segmented control (framed on 1 bit, the chosen
//! one inverse), the status in segments. Each thing that can be pressed has a [`Zone`] naming
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
    /// A key's letter: the theme's accent.
    Accent,
    /// The site's name on the bar: on the accent.
    Brand,
    /// The divider between the choices of one segmented control.
    Seam,
    /// The column's ground, under its cards: a pixel pattern on 1 bit.
    Ground,
}

impl Role {
    /// Part of a control: a button, the chosen one, a divider between.
    fn control(self) -> bool {
        matches!(self, Role::Button | Role::On | Role::Seam)
    }
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

/// A menu on the bar: what it sets (`style`),
/// its value (with `▾`) and the options it drops down.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub id: String,
    /// What it sets, quieter, before its value.
    pub title: String,
    /// The key that opens it, in the accent where the title has it.
    pub key: Option<char>,
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
    /// Buttons side by side: one segmented control.
    Row(Vec<Button>),
    /// A key and what it does, in two columns.
    Key(String, String),
    Blank,
}

/// The interface to draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    /// The site's name, first on the bar.
    pub brand: String,
    /// The bar's left: the site's own links.
    pub links: Vec<Button>,
    pub menus: Vec<Menu>,
    /// The bar's right, after its menus.
    pub right: Vec<Button>,
    /// The menu dropped down, by id.
    pub open: Option<String>,
    /// The column's lines, when it is open.
    pub column: Option<Vec<Line>>,
    /// The rows the column is scrolled by (held to what it has).
    pub scroll: usize,
    /// Floating top right while the column is closed.
    pub tools: Vec<Button>,
    /// The status line: what is shown, in segments.
    pub status: Vec<String>,
    /// Keys and what they do, at the status line's right.
    pub keys: Vec<(String, String)>,
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
    /// The cells across left of the column, where the map is (all of them
    /// while it is shut).
    pub free: usize,
    /// The rows the column is scrolled by, held to what it has.
    pub scroll: usize,
    /// The column's cards, in cells, as they show.
    pub cards: Vec<Card>,
}

/// A card on the column's ground, in cells; cut by the view's edge at its
/// top or bottom when it runs past it.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub open_top: bool,
    pub open_bottom: bool,
}

/// Narrower than this many cells (a phone), the column is a sheet over the
/// whole screen: a third of it would cut every card's line short.
const NARROW: usize = 72;

/// The column's width in cells on a screen `width` across.
fn column_width(width: usize) -> usize {
    if width < NARROW {
        width
    } else {
        (width / 3).clamp(22, 34)
    }
}

impl Chrome {
    /// `model` laid out on `width` × `height` cells.
    pub fn lay(model: &Model, width: usize, height: usize) -> Chrome {
        let mut c = Chrome { width, height, cells: vec![None; width * height], zones: Vec::new(), free: width, scroll: 0, cards: Vec::new() };
        if width == 0 || height == 0 {
            return c;
        }
        c.bar(model);
        if height >= 3 {
            c.status(model);
        }
        match &model.column {
            Some(lines) if height >= 4 => c.column(lines, model.scroll),
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
        let menu_width = |m: &Menu, short: bool| if short || m.title.is_empty() { width(if short { &m.short } else { &m.label }) } else { width(&m.label) + m.title.chars().count() + 1 };
        let brand = if model.brand.is_empty() { 0 } else { width(&model.brand) + 1 };
        let links: usize = model.links.iter().map(|b| width(&b.label) + 1).sum();
        let right: usize = model.right.iter().map(|b| width(&b.label) + 1).sum();
        let menus = |short: bool| -> usize { model.menus.iter().map(|m| menu_width(m, short) + 1).sum::<usize>() + 2 };
        let short = brand + links + menus(false) + right > w;
        // On a narrow bar the menus stay, short; the outward links go
        // first, then the page's links, then the brand.
        let fits = |b: bool, l: bool, r: bool| b as usize * brand + l as usize * links + menus(true) + r as usize * right <= w;
        let (show_brand, show_links, show_right) =
            [(true, true, true), (true, true, false), (true, false, false), (false, false, false)].into_iter().find(|&(b, l, r)| fits(b, l, r)).unwrap_or((false, false, false));
        let mut x = 0;
        if brand > 0 && show_brand {
            x += self.write(x, 0, &format!(" {} ", model.brand), Role::Brand, w) + 1;
        } else {
            x = 1;
        }
        if show_links {
            for b in &model.links {
                x += self.button(x, 0, b, Role::Bar, w) + 1;
            }
        }
        let (left, right_menus): (Vec<&Menu>, Vec<&Menu>) = model.menus.iter().partition(|m| !m.right);
        if !left.is_empty() {
            x += self.write(x, 0, "│", Role::Rule, w) + 1;
        }
        for m in left {
            x += self.menu_label(x, m, short, model.open.as_deref() == Some(&m.id), w) + 1;
        }
        // The right, from the edge in.
        let mut rx = w;
        for b in model.right.iter().rev().filter(|_| show_right) {
            let n = width(&b.label);
            if rx < x + n {
                break;
            }
            rx -= n;
            self.button(rx, 0, b, Role::Bar, w);
            rx = rx.saturating_sub(1);
        }
        for m in right_menus.iter().rev() {
            let n = menu_width(m, short);
            if rx < x + n {
                break;
            }
            rx -= n;
            self.menu_label(rx, m, short, model.open.as_deref() == Some(&m.id), w);
            rx = rx.saturating_sub(1);
        }
    }

    /// A menu on the bar: ` style terminal ▾ `, the title's key letter in
    /// the accent and the rest of it quieter; the value alone
    /// when the bar is short. Inverse while it is open.
    fn menu_label(&mut self, x: usize, m: &Menu, short: bool, open: bool, end: usize) -> usize {
        let action = format!("open:{}", m.id);
        if short || m.title.is_empty() {
            let b = Button::new(if short { &m.short } else { &m.label }, action).on(open);
            return self.button(x, 0, &b, Role::Bar, end);
        }
        let (rest, key, quiet) = if open { (Role::On, Role::On, Role::On) } else { (Role::Bar, Role::Accent, Role::Dim) };
        let mut n = self.write(x, 0, " ", rest, end);
        let mut keyed = false;
        for ch in m.title.chars() {
            let is_key = !keyed && Some(ch) == m.key;
            keyed |= is_key;
            n += self.write(x + n, 0, &ch.to_string(), if is_key { key } else { quiet }, end);
        }
        n += self.write(x + n, 0, &format!(" {} ", m.label), rest, end);
        self.zone(x, 0, n, 1, &action);
        n
    }

    /// A menu dropped down under its label.
    fn menu(&mut self, m: &Menu) {
        let at = self.zones.iter().find(|z| z.y == 0 && z.action == format!("open:{}", m.id)).map(|z| z.x);
        let Some(at) = at else { return };
        let inner = m.options.iter().map(|o| o.label.chars().count() + 4).max().unwrap_or(4);
        let w = (inner + 2).min(self.width);
        let label = self.zones.iter().find(|z| z.y == 0 && z.action == format!("open:{}", m.id)).map(|z| z.w).unwrap_or(0);
        let x = if m.right { (at + label).saturating_sub(w).min(self.width - w) } else { at.min(self.width - w) };
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

    /// The status line: its segments divided by rules; the keys at its
    /// right where they fit, each key in the accent and its deed quieter.
    fn status(&mut self, model: &Model) {
        let (w, y) = (self.width, self.height - 1);
        self.fill(0, y, w, 1, Role::Bar);
        self.zone(0, y, w, 1, "none");
        let keys: usize = model.keys.iter().map(|(k, d)| k.chars().count() + d.chars().count() + 3).sum();
        let status: usize = model.status.iter().map(|s| s.chars().count() + 3).sum();
        let fits = !model.keys.is_empty() && status + keys + 2 <= w;
        let room = if fits { w - keys - 1 } else { w };
        let mut x = 1;
        for (i, seg) in model.status.iter().enumerate() {
            if i > 0 {
                x += self.write(x, y, " │ ", Role::Rule, room);
            }
            x += self.write(x, y, seg, Role::Bar, room);
        }
        if fits {
            let mut x = w - keys;
            for (k, d) in &model.keys {
                x += self.write(x, y, k, Role::Accent, w);
                x += self.write(x, y, &format!(" {}  ", d), Role::Dim, w);
            }
        }
    }

    /// The column: a header with its close button, then its sections, each
    /// a card on the column's ground, scrolled by `scroll` rows when they
    /// are more than it holds, a scrollbar at its right edge then.
    fn column(&mut self, lines: &[Line], scroll: usize) {
        let cw = column_width(self.width);
        let x0 = self.width - cw;
        // A sheet over the whole screen leaves the map whole under it.
        self.free = if x0 == 0 { self.width } else { x0 };
        let (top, bottom) = (1, self.height - 1);
        self.fill(x0, top, cw, bottom - top, Role::Ground);
        for y in top..bottom {
            self.put(x0, y, '│', Role::Rule);
        }
        self.zone(x0, top, cw, bottom - top, "none");
        // The header, fixed: what it is, and × to close it.
        self.fill(x0 + 1, top, cw - 1, 1, Role::Panel);
        self.write(x0 + 2, top, "settings", Role::Dim, self.width - 1);
        self.button(self.width - 4, top, &Button::new("×", "column"), Role::Panel, self.width);
        // The sections laid out whole on a sheet, then the part in view.
        let sheet = Chrome::sheet(lines, self.width, x0);
        let (view_top, view) = (top + 2, bottom.saturating_sub(top + 2));
        let most = sheet.height.saturating_sub(view);
        self.scroll = scroll.min(most);
        for vy in self.scroll..(self.scroll + view).min(sheet.height) {
            let y = view_top + vy - self.scroll;
            for x in x0 + 1..self.width {
                if let Some(c) = sheet.cell(x, vy) {
                    self.cells[y * self.width + x] = Some(c);
                }
            }
        }
        for z in &sheet.zones {
            let (z0, z1) = (z.y.max(self.scroll), (z.y + z.h).min(self.scroll + view));
            if z0 < z1 {
                self.zone(z.x, view_top + z0 - self.scroll, z.w, z1 - z0, &z.action);
            }
        }
        for c in &sheet.cards {
            let (c0, c1) = (c.y as isize - self.scroll as isize, (c.y + c.h) as isize - self.scroll as isize);
            if c1 > 0 && c0 < view as isize {
                // Cut where it leaves the view: its edge there is the view's.
                let (a, b) = (c0.max(0) as usize, (c1 as usize).min(view));
                self.cards.push(Card { x: c.x, y: view_top + a, w: c.w, h: b - a, open_top: c0 < 0, open_bottom: c1 as usize > view });
            }
        }
        // The scrollbar: a track at the edge, the thumb where the view is;
        // a press above or below it turns a page.
        if most > 0 && view > 2 {
            let x = self.width - 1;
            let thumb = (view * view / sheet.height).max(1);
            let at = (self.scroll * (view - thumb)).div_ceil(most.max(1)).min(view - thumb);
            for i in 0..view {
                let on = (at..at + thumb).contains(&i);
                self.put(x, view_top + i, if on { '█' } else { '│' }, if on { Role::Dim } else { Role::Ground });
            }
            self.zone(x, view_top, 1, at, &format!("scroll:-{view}"));
            self.zone(x, view_top + at + thumb, 1, view - at - thumb, &format!("scroll:{view}"));
        }
    }

    /// The column's sections as cards, laid out on a sheet as tall as they
    /// need, `x0` the column's left.
    fn sheet(lines: &[Line], width: usize, x0: usize) -> Chrome {
        let mut s = Chrome { width, height: 2000, cells: vec![None; width * 2000], zones: Vec::new(), free: width, scroll: 0, cards: Vec::new() };
        // A card a cell in from the column's sides, its text two cells further.
        let (card_x, card_w) = (x0 + 2, width.saturating_sub(x0 + 4));
        let (left, end) = (card_x + 2, card_x + card_w - 2);
        let key = lines.iter().filter_map(|l| if let Line::Key(k, _) = l { Some(k.chars().count()) } else { None }).max().unwrap_or(0);
        let mut y = 0;
        let mut card: Option<usize> = None;
        let close = |s: &mut Chrome, card: &mut Option<usize>, y: &mut usize| {
            if let Some(from) = card.take() {
                // A blank row closes the card; another, the ground, parts it
                // from the next.
                s.fill(card_x, *y, card_w, 1, Role::Panel);
                *y += 1;
                s.cards.push(Card { x: card_x, y: from, w: card_w, h: *y - from, open_top: false, open_bottom: false });
                *y += 1;
            }
        };
        let row = |s: &mut Chrome, y: usize| s.fill(card_x, y, card_w, 1, Role::Panel);
        for line in lines {
            if let Line::Head(_) = line {
                close(&mut s, &mut card, &mut y);
                card = Some(y);
                row(&mut s, y);
                y += 1;
            }
            match line {
                Line::Head(t) => {
                    row(&mut s, y);
                    let n = s.write(left, y, &t.to_uppercase(), Role::Dim, end);
                    for x in left + n + 1..end {
                        s.put(x, y, '─', Role::Rule);
                    }
                }
                Line::Text(t) => {
                    row(&mut s, y);
                    s.write(left, y, t, Role::Panel, end);
                }
                Line::Dim(t) => {
                    row(&mut s, y);
                    s.write(left, y, t, Role::Dim, end);
                }
                Line::Key(k, d) => {
                    row(&mut s, y);
                    s.write(left, y, k, Role::Accent, end);
                    s.write(left + key + 2, y, d, Role::Dim, end);
                }
                Line::Row(buttons) => {
                    // One segmented control, wrapped where it is too wide.
                    row(&mut s, y);
                    let mut x = left;
                    for b in buttons {
                        let n = b.label.chars().count() + 2;
                        if x > left && x + 1 + n > end {
                            row(&mut s, y + 1);
                            y += 2;
                            row(&mut s, y);
                            x = left;
                        }
                        if x > left {
                            x += s.write(x, y, "│", Role::Seam, end);
                        }
                        x += s.button(x, y, b, Role::Button, end);
                    }
                }
                Line::Blank => row(&mut s, y),
            }
            y += 1;
        }
        close(&mut s, &mut card, &mut y);
        s.height = y;
        s
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

/// A chrome's colours on a text screen: a theme's paper, ink and accent,
/// the rest mixed from them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub paper: Rgb,
    pub ink: Rgb,
    /// Keys' letters and the site's name.
    pub accent: Rgb,
}

/// The themes of draw.land's page, by name: `paper`, `ink`, `silk`,
/// `abyss`, `ember`, `phosphor`.
pub const THEMES: &[(&str, Palette)] = &[
    ("paper", Palette { paper: Rgb(0xff, 0xff, 0xff), ink: Rgb(0x00, 0x00, 0x00), accent: Rgb(0xd0, 0x3a, 0x1a) }),
    ("ink", Palette { paper: Rgb(0x0a, 0x0b, 0x0d), ink: Rgb(0xee, 0xf1, 0xf6), accent: Rgb(0xff, 0xb4, 0x3a) }),
    ("silk", Palette { paper: Rgb(0xf7, 0xf6, 0xf2), ink: Rgb(0x5f, 0x5d, 0x58), accent: Rgb(0xb4, 0x5a, 0x2c) }),
    ("abyss", Palette { paper: Rgb(0x0b, 0x23, 0x26), ink: Rgb(0xe9, 0xdf, 0xc8), accent: Rgb(0x5f, 0xd3, 0xc4) }),
    ("ember", Palette { paper: Rgb(0x12, 0x0e, 0x06), ink: Rgb(0xff, 0xbf, 0x3a), accent: Rgb(0xff, 0x6a, 0x2a) }),
    ("phosphor", Palette { paper: Rgb(0x0a, 0x0f, 0x0a), ink: Rgb(0x41, 0xff, 0x41), accent: Rgb(0xc8, 0xff, 0xc8) }),
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

    /// The bar's ground, and the column's and menus' (a shade off the map).
    fn bar(&self) -> Rgb {
        self.mix(0.08)
    }

    /// Cards, menus and the tools' box.
    fn panel(&self) -> Rgb {
        self.mix(0.09)
    }

    /// A role's ink and paper, on the column.
    pub fn colours(&self, role: Role) -> (Rgb, Rgb) {
        match role {
            Role::Bar => (self.mix(0.92), self.bar()),
            Role::Button => (self.mix(0.92), self.mix(0.2)),
            Role::Seam => (self.mix(0.4), self.mix(0.2)),
            Role::Ground => (self.mix(0.3), self.mix(0.02)),
            Role::On => (self.paper, self.ink),
            Role::Panel => (self.ink, self.panel()),
            Role::Dim => (self.mix(0.5), self.panel()),
            Role::Rule => (self.mix(0.22), self.panel()),
            Role::Accent => (self.accent, self.panel()),
            Role::Brand => (self.paper, self.accent),
        }
    }

    /// The colours of a role's cell where it sits: quiet words, rules and
    /// keys on the bar or the status line keep the bar's ground.
    pub fn at(&self, role: Role, row: usize, height: usize) -> (Rgb, Rgb) {
        let (fg, bg) = self.colours(role);
        let on_bar = row == 0 || row + 1 == height;
        match role {
            Role::Dim | Role::Rule | Role::Accent | Role::Panel if on_bar => (fg, self.bar()),
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
        if let Some(black) = self.card_edge(px, py) {
            return Some(black);
        }
        Some(match role {
            Role::On | Role::Brand => !ink,
            // The column's ground: one pixel in eight, staggered row to row,
            // only where no line is drawn.
            Role::Ground => ink || (ch == ' ' && py % 2 == 0 && (px + py / 2 * 2) % 4 == 0),
            // A control is framed, the frame broken where a letter's tail
            // reaches it and its corners rounded; the chosen one inverse.
            Role::Button | Role::Seam if cy > 0 => ink || self.frame(cx, cy, x, y),
            // A key's letter is underlined, broken where a tail reaches it.
            Role::Accent => ink || (y + 1 == fh && !self.tail(ch, x)) || edge,
            // One bit has no grey: quieter words are inked like the rest (a
            // dither of a 6 × 12 glyph cannot be read).
            _ => ink || edge,
        })
    }

    /// A card's frame (`Some(true)`), the paper just inside it
    /// (`Some(false)`, so no glyph touches the frame), or its shadow (two
    /// pixels down and right, `Some(true)`); `None` elsewhere.
    fn card_edge(&self, px: usize, py: usize) -> Option<bool> {
        let (fw, fh) = (self.font.width, self.font.height);
        for c in &self.chrome.cards {
            let (x0, y0, x1, y1) = (c.x * fw, c.y * fh, (c.x + c.w) * fw, (c.y + c.h) * fh);
            let inside = (x0..x1).contains(&px) && (y0..y1).contains(&py);
            if inside {
                let side = px == x0 || px + 1 == x1;
                let top = py == y0 && !c.open_top;
                let bottom = py + 1 == y1 && !c.open_bottom;
                if side || top || bottom {
                    return Some(true);
                }
                continue;
            }
            let right = (x1..x1 + 2).contains(&px) && py >= y0 + 3 && (py < y1 + 2 || c.open_bottom) && py < y1 + 2;
            let below = !c.open_bottom && (y1..y1 + 2).contains(&py) && px >= x0 + 3 && px < x1 + 2;
            if right || below {
                return Some(true);
            }
        }
        None
    }

    /// Whether `(x, y)` in cell `(cx, cy)` is on its control's frame.
    fn frame(&self, cx: usize, cy: usize, x: usize, y: usize) -> bool {
        let (fw, fh) = (self.font.width, self.font.height);
        let c = self.chrome;
        let part = |dx: isize| {
            let nx = cx as isize + dx;
            nx >= 0 && (nx as usize) < c.width && c.cell(nx as usize, cy).is_some_and(|(_, r)| r.control())
        };
        let (first, last) = (!part(-1), !part(1));
        let (left, right) = (first && x == 0, last && x + 1 == fw);
        let (top, bottom) = (y == 0, y + 1 == fh);
        // Rounded: no corner pixel.
        if (left || right) && (top || bottom) {
            return false;
        }
        let tail = bottom && c.cell(cx, cy).is_some_and(|(ch, _)| self.tail(ch, x));
        left || right || top || (bottom && !tail)
    }

    /// Whether a letter's tail comes near the cell's foot at `x`.
    fn tail(&self, ch: char, x: usize) -> bool {
        let fh = self.font.height;
        (x.saturating_sub(1)..=(x + 1).min(self.font.width - 1)).any(|xx| (fh.saturating_sub(3)..fh).any(|yy| glyph(self.font, ch, xx, yy)))
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
        '×' if !font.has(ch) => {
            let (cx, cy, r) = (w / 2.0, h * 0.5, w * 0.3);
            ((fx - cx).abs() - (fy - cy).abs()).abs() < 0.6 && (fx - cx).abs() <= r
        }
        '█' if !font.has(ch) => true,
        '·' if !font.has(ch) => (fx - w / 2.0).abs() < 0.6 && (fy - h * 0.5).abs() < 0.6,
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
            brand: "draw.land".into(),
            links: vec![Button::new("map", "link:/").on(true), Button::new("docs", "link:/docs")],
            menus: vec![
                Menu {
                    id: "style".into(),
                    title: "style".into(),
                    key: Some('s'),
                    label: "terminal ▾".into(),
                    short: "terminal▾".into(),
                    options: vec![Button::new("terminal", "style:terminal").on(true), Button::new("1-bit", "style:1-bit")],
                    right: false,
                },
                Menu { id: "theme".into(), title: "theme".into(), key: Some('t'), label: "ink ▾".into(), short: "▾".into(), options: vec![Button::new("paper", "theme:paper")], right: true },
            ],
            right: vec![Button::new("GitHub ↗", "link:https://github.com/piLeoni/drawland")],
            open: None,
            column: None,
            scroll: 0,
            tools: vec![Button::new("+", "zoom:1"), Button::new("−", "zoom:-1"), Button::new("≡", "column")],
            status: vec!["45.43750, 12.33580".into(), "8 m a character".into()],
            keys: vec![("s".into(), "style".into()), ("p".into(), "panel".into())],
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
        assert!(bar.starts_with(" draw.land   map   docs  │  style terminal ▾ "), "{bar}");
        assert!(bar.trim_end().ends_with("theme ink ▾   GitHub ↗"), "{bar}");
        assert_eq!(c.cell(0, 0), Some((' ', Role::Brand)));
        assert_eq!(c.cell(12, 0), Some((' ', Role::On)), "the page shown is inverse");
        assert_eq!(c.cell(col(&c, 0, "style"), 0), Some(('s', Role::Accent)), "a menu's key in the accent");
        assert_eq!(c.hit(col(&c, 0, "docs"), 0), Some("link:/docs"));
        let status = row(&c, 23);
        assert!(status.contains("45.43750, 12.33580 │ 8 m a character") && status.contains("s style  p panel"), "{status}");
        // The map shows everywhere else but the tools.
        assert_eq!(c.cell(10, 10), None);
        assert_eq!(c.hit(78 - 2, 2), Some("column"), "{}", row(&c, 2));
    }

    #[test]
    fn a_menu_drops_down_under_its_label_and_its_options_are_pressed() {
        let mut m = model();
        m.open = Some("style".into());
        let c = Chrome::lay(&m, 80, 24);
        let at = col(&c, 0, "style terminal") - 1;
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
        assert_eq!(c.cards.len(), 1, "the section a card");
        assert!(row(&c, 1).contains("settings") && row(&c, 1).contains(" × "), "{}", row(&c, 1));
        assert_eq!(c.hit(89 - 2, 1), Some("column"), "× closes it");
        assert!(row(&c, 4).contains("SCREEN ──"), "{}", row(&c, 4));
        assert!(row(&c, 5).contains(" 10 │ 12 "), "one segmented control: {}", row(&c, 5));
        assert_eq!(c.cell(col(&c, 5, "│ 12"), 5), Some(('│', Role::Seam)));
        assert!(c.zones.iter().all(|z| z.action != "column" || z.y == 1), "no floating tools");
        assert_eq!(c.hit(x0 + 5, 5), Some("size:10"));
    }

    #[test]
    fn on_a_phone_the_menus_stay_and_the_column_is_a_sheet() {
        // 46 characters: a phone's terminal page. The links go before the
        // menus; the theme stays.
        let c = Chrome::lay(&model(), 46, 50);
        let bar = row(&c, 0);
        assert!(bar.contains("terminal▾") && bar.contains("▾"), "{bar}");
        assert!(c.zones.iter().any(|z| z.action == "open:theme"), "the theme menu stays: {bar}");
        assert!(!bar.contains("GitHub"), "{bar}");
        // The column covers the screen, the map whole under it.
        let mut m = model();
        m.column = Some(vec![Line::Head("place".into()), Line::Text("45.43750, 12.33580".into())]);
        let c = Chrome::lay(&m, 46, 50);
        assert_eq!(c.free, 46);
        assert!((1..10).any(|y| row(&c, y).contains("45.43750, 12.33580")), "the card's line whole");
    }

    #[test]
    fn a_long_column_scrolls_and_holds_to_what_it_has() {
        let mut m = model();
        m.column = Some((0..12).flat_map(|i| vec![Line::Head(format!("part {i}")), Line::Text("words".into()), Line::Row(vec![Button::new("a", format!("a:{i}"))])]).collect());
        m.scroll = 10_000;
        let c = Chrome::lay(&m, 90, 20);
        // Twelve cards of five rows (title, words, choice, closing row, gap):
        // the last row in view is the last card's closing row.
        assert!(c.scroll > 0 && c.scroll < 10_000, "held: {}", c.scroll);
        assert!((0..20).any(|y| row(&c, y).contains("PART 11")), "the end in view");
        assert!(!(0..20).any(|y| row(&c, y).contains("PART 0 ")), "the start scrolled off");
        assert_eq!(c.cell(89, 18).map(|(_, r)| r), Some(Role::Dim), "the thumb at the bottom");
        // A press on the track above the thumb turns a page back.
        assert!(c.hit(89, 4).is_some_and(|a| a.starts_with("scroll:-")));
        // A card cut by the view's top is open there.
        assert!(c.cards.first().is_some_and(|k| k.open_top || k.y > 3));
    }

    #[test]
    fn a_narrow_bar_uses_the_short_labels() {
        let c = Chrome::lay(&model(), 34, 10);
        let bar = row(&c, 0);
        assert!(bar.contains("terminal▾"), "{bar}");
    }

    #[test]
    fn on_pixels_the_bar_is_ruled_and_the_map_shows_below_it() {
        let c = Chrome::lay(&model(), 80, 20);
        let font = crate::font::font("spleen-6x12").unwrap();
        let p = Pixels::new(&c, font);
        assert_eq!(p.at(150, 11), Some(true), "the rule under the bar");
        assert_eq!(p.at(100, 5 * 12), None, "the map");
        // The site's name and the page shown are inverse: their padding is ink.
        assert_eq!(p.at(2, 5), Some(true));
        assert_eq!(p.at(12 * 6 + 2, 5), Some(true));
    }
}
