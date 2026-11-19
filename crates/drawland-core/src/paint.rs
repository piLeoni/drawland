//! A screen: the output as a grid of pixels (a 1-bit bitmap, every pixel
//! black or white) or of character cells (a terminal),
//! and the `paint` entries that cover it.
//!
//! The pen plotter's `draw` entries are strokes on paper; a screen has none
//! of them. A `paint` entry says how one area, line set or set of named
//! features covers the grid: a pattern or a character for an area, a 1-pixel
//! line or box-drawing characters for lines, a pixel font or plain text for
//! names, and the clear `halo` it keeps round its ink in what lies under it.
//! The engine evaluates the graph as usual and hands each entry its
//! geometry in the output's units ([`crate::grammar_run::paint`]); the grid
//! itself is drawn by `drawland-grid`.
//!
//! On a screen the output's unit is not the millimetre but the screen's own:
//! a **pixel** on a bitmap, a **character** on text. Every length a profile
//! writes (a buffer, an offset, `page.width`) counts them, and the scale is
//! the ground one of them spans (`scale: "3m"`). A character is one unit
//! across and two down (a row is as tall as two characters are wide), so a
//! circle is still round on the screen.

use crate::grammar::{At, Target};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A terminal cell is this many dots across and down (a Braille character's
/// grid): a cell twice as tall as it is wide, so a dot is half a character.
pub const CELL_DOTS: (u32, u32) = (2, 4);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenKind {
    /// Black and white pixels, written as PNG or PBM.
    Bitmap,
    /// Character cells, each a character and two colours, written as plain
    /// text, ANSI (a terminal) or HTML.
    Text,
}

/// `output({ screen: { kind: "text", width: 120, height: 40 } })`: the
/// output is a screen this many pixels (a bitmap) or cells (text) across and
/// down, and its size in units follows from it ([`Screen::size`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    pub kind: ScreenKind,
    /// Pixels or columns.
    pub width: u32,
    /// Pixels or rows.
    pub height: u32,
    /// White ink on black (an OLED): the bitmap's bits flipped as it is
    /// written. Text ignores it; its colours say.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
}

impl Screen {
    /// The finest grid painted: pixels, or a cell's dots.
    pub fn dots(&self) -> (u32, u32) {
        match self.kind {
            ScreenKind::Bitmap => (self.width, self.height),
            ScreenKind::Text => (self.width * CELL_DOTS.0, self.height * CELL_DOTS.1),
        }
    }

    /// A cell of the grid in the output's units, across and down: a pixel
    /// is one by one, a character one by two.
    pub fn cell(&self) -> (f64, f64) {
        match self.kind {
            ScreenKind::Bitmap => (1.0, 1.0),
            ScreenKind::Text => (1.0, 2.0),
        }
    }

    /// A dot in the output's units: a pixel, or half a character.
    pub fn dot(&self) -> f64 {
        match self.kind {
            ScreenKind::Bitmap => 1.0,
            ScreenKind::Text => 1.0 / CELL_DOTS.0 as f64,
        }
    }

    /// The output's size in its units: pixels, or columns and twice the rows.
    pub fn size(&self) -> (f64, f64) {
        let (w, h) = self.cell();
        (self.width as f64 * w, self.height as f64 * h)
    }

    pub fn validate(&self) -> Result<(), String> {
        const MAX: u32 = 8192;
        if self.width == 0 || self.height == 0 || self.width > MAX || self.height > MAX {
            return Err(format!("output.screen: width and height are 1 to {MAX}, got {} × {}", self.width, self.height));
        }
        Ok(())
    }
}

/// A colour, `#rgb` or `#rrggbb`, or one of the sixteen terminal names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

const NAMED: &[(&str, Rgb)] = &[
    ("black", Rgb(0, 0, 0)),
    ("white", Rgb(255, 255, 255)),
    ("red", Rgb(205, 49, 49)),
    ("green", Rgb(13, 188, 121)),
    ("yellow", Rgb(229, 229, 16)),
    ("blue", Rgb(36, 114, 200)),
    ("magenta", Rgb(188, 63, 188)),
    ("cyan", Rgb(17, 168, 205)),
    ("gray", Rgb(128, 128, 128)),
    ("grey", Rgb(128, 128, 128)),
    ("brightred", Rgb(241, 76, 76)),
    ("brightgreen", Rgb(35, 209, 139)),
    ("brightyellow", Rgb(245, 245, 67)),
    ("brightblue", Rgb(59, 142, 234)),
    ("brightmagenta", Rgb(214, 112, 214)),
    ("brightcyan", Rgb(41, 184, 219)),
];

impl Rgb {
    pub fn parse(text: &str) -> Result<Rgb, String> {
        let t = text.trim();
        if let Some(hex) = t.strip_prefix('#') {
            let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
            let b = hex.as_bytes();
            let d: Option<Vec<u8>> = b.iter().map(|&c| digit(c)).collect();
            match (d, b.len()) {
                (Some(d), 3) => return Ok(Rgb(d[0] * 17, d[1] * 17, d[2] * 17)),
                (Some(d), 6) => return Ok(Rgb(d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5])),
                _ => {}
            }
        }
        let lower = t.to_ascii_lowercase();
        NAMED
            .iter()
            .find(|(n, _)| *n == lower)
            .map(|(_, c)| *c)
            .ok_or_else(|| format!("colour {text:?}: write #rgb, #rrggbb or a terminal name (black, red, blue, brightcyan, gray…)"))
    }

    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Rgb, D::Error> {
        let text = String::deserialize(d)?;
        Rgb::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// The bitmap's two inks: black on the white screen, or white (to cut a
/// road out of black water).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ink {
    #[default]
    Black,
    White,
}

impl Ink {
    pub fn is_default(&self) -> bool {
        *self == Ink::Black
    }
}

/// The clear space a layer keeps round its ink in the layers under it, in
/// pixels or cells: one for both directions, or `[across, down]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Halo(pub u32, pub u32);

impl Halo {
    pub fn is_none(&self) -> bool {
        self.0 == 0 && self.1 == 0
    }
}

impl Serialize for Halo {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0 == self.1 {
            s.serialize_u32(self.0)
        } else {
            [self.0, self.1].serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Halo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Halo, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(u32),
            Two([u32; 2]),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::One(n) => Halo(n, n),
            Raw::Two([x, y]) => Halo(x, y),
        })
    }
}

/// Characters tiled over an area: one string repeated along each row, or
/// rows of them (`["~ ", " ~"]`), row after row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile(pub Vec<Vec<char>>);

impl Serialize for Tile {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let rows: Vec<String> = self.0.iter().map(|r| r.iter().collect()).collect();
        if rows.len() == 1 {
            rows[0].serialize(s)
        } else {
            rows.serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Tile {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Tile, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(String),
            Rows(Vec<String>),
        }
        let rows = match Raw::deserialize(d)? {
            Raw::One(s) => vec![s],
            Raw::Rows(r) => r,
        };
        let rows: Vec<Vec<char>> = rows.into_iter().map(|r| r.chars().collect()).collect();
        if rows.is_empty() || rows.iter().any(|r| r.is_empty()) {
            return Err(serde::de::Error::custom("chars: a string, or rows of strings, none empty"));
        }
        if rows.iter().flatten().any(|c| c.is_control()) {
            return Err(serde::de::Error::custom("chars: no control characters"));
        }
        Ok(Tile(rows))
    }
}

/// How an area covers the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct FillPaint {
    /// A pixel pattern by name (`solid`, `dots`, `grid`, `hatch`, `waves`…),
    /// tiled from a fixed point of the world so it stays put as the map
    /// moves. On text it is drawn in Braille dots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// A grey, 0 (white) to 1 (black), dithered: ordered (Bayer), so it
    /// holds still as the map moves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<f64>,
    /// Text: characters tiled over the area.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chars: Option<Tile>,
    /// Text: the area in quarter-cell blocks (`▖▝▀▙█`), its edge twice as
    /// fine as a cell.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub blocks: bool,
    /// Text: the characters' colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    /// Text: the cells' colour under the characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Rgb>,
    /// Bitmap: the ink the pattern is drawn in.
    #[serde(skip_serializing_if = "Ink::is_default")]
    pub ink: Ink,
    /// Off: only the pattern's ink is laid, and what is under the area shows
    /// through its gaps. On (the default): the gaps are cleared.
    #[serde(skip_serializing_if = "is_true")]
    pub opaque: bool,
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Default for FillPaint {
    fn default() -> Self {
        FillPaint {
            pattern: None,
            tone: None,
            chars: None,
            blocks: false,
            color: None,
            background: None,
            ink: Ink::Black,
            opaque: true,
        }
    }
}

/// The patterns `fill.pattern` names, each 8 × 8.
pub const PATTERNS: &[&str] = &[
    "solid", "dots", "sparse", "grid", "lines", "columns", "hatch", "backhatch", "cross", "checker", "bricks", "waves",
    "stipple", "diamonds",
];

impl FillPaint {
    fn validate(&self) -> Result<(), String> {
        if let Some(p) = &self.pattern {
            if !PATTERNS.contains(&p.as_str()) {
                return Err(format!("fill.pattern {p:?}: one of {}", PATTERNS.join(", ")));
            }
        }
        if let Some(t) = self.tone {
            if !(0.0..=1.0).contains(&t) {
                return Err(format!("fill.tone is 0 (white) to 1 (black), got {t}"));
            }
        }
        let ways = [self.pattern.is_some(), self.tone.is_some(), self.chars.is_some(), self.blocks].iter().filter(|b| **b).count();
        if ways > 1 {
            return Err("fill: one of pattern, tone, chars or blocks".into());
        }
        if ways == 0 && self.background.is_none() {
            return Err("fill: give a pattern, tone, chars, blocks or a background".into());
        }
        Ok(())
    }
}

impl Default for LinePaint {
    fn default() -> Self {
        LinePaint { width: 1, dash: None, ink: Ink::Black, style: LineStyle::Braille, char: None, color: None, background: None }
    }
}

/// How lines are drawn in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineStyle {
    /// Braille dots, two across and four down a cell: the finest line.
    #[default]
    Braille,
    /// Box drawing, `─│┌┼`: lines that meet are joined.
    Box,
    /// Heavy box drawing, `━┃┏╋`.
    Heavy,
    /// Double box drawing, `═║╔╬`.
    Double,
    /// Rounded box drawing, `─│╭╯`.
    Round,
    /// `-`, `|`, `/`, `\`, and `+` where they meet.
    Ascii,
}

impl LineStyle {
    fn is_default(&self) -> bool {
        *self == LineStyle::Braille
    }
}

/// How lines cover the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct LinePaint {
    /// Bitmap: pixels across, no anti-aliasing.
    #[serde(skip_serializing_if = "is_one")]
    pub width: u32,
    /// Bitmap: pixels on, pixels off, along the line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Ink::is_default")]
    pub ink: Ink,
    /// Text: what the line is drawn with.
    #[serde(skip_serializing_if = "LineStyle::is_default")]
    pub style: LineStyle,
    /// Text: one character in every cell the line crosses, over `style`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char: Option<char>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Rgb>,
}

fn is_one(n: &u32) -> bool {
    *n == 1
}

impl LinePaint {
    fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.width > 64 {
            return Err(format!("line.width is 1 to 64 pixels, got {}", self.width));
        }
        if let Some([on, off]) = self.dash {
            if on == 0 || off == 0 {
                return Err("line.dash is [on, off] in pixels, both at least 1".into());
            }
        }
        if self.char.is_some_and(char::is_control) {
            return Err("line.char: not a control character".into());
        }
        Ok(())
    }
}

/// How names are written on the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct TextPaint {
    /// The tags the name is read from, the first one present.
    pub keys: Vec<String>,
    /// Write the names in capitals.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub upper: bool,
    /// Bitmap: a pixel font, `spleen-5x8`, `spleen-6x12`, `spleen-8x16` or
    /// `spleen-12x24`.
    pub font: String,
    /// A mark at a place's point, its name set beside it: a character on
    /// text (`"•"`), a dot this many pixels across on a bitmap (`3`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<Marker>,
    /// Pixels or cells between two labels of the same name.
    pub spacing: u32,
    /// Clear pixels or cells kept between this layer's names and any other's.
    pub padding: u32,
    #[serde(skip_serializing_if = "Ink::is_default")]
    pub ink: Ink,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Rgb>,
}

/// A place's mark: a dot so many pixels across, or a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Dot(u32),
    Char(char),
}

impl Serialize for Marker {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Marker::Dot(n) => s.serialize_u32(*n),
            Marker::Char(c) => s.serialize_str(&c.to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for Marker {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Marker, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Dot(u32),
            Char(String),
        }
        match Raw::deserialize(d)? {
            Raw::Dot(n) if (1..=32).contains(&n) => Ok(Marker::Dot(n)),
            Raw::Char(s) if s.chars().count() == 1 && !s.chars().next().unwrap().is_control() => {
                Ok(Marker::Char(s.chars().next().unwrap()))
            }
            _ => Err(serde::de::Error::custom("marker: a dot's size in pixels (1 to 32) or one character")),
        }
    }
}

pub const FONTS: &[&str] = &["spleen-5x8", "spleen-6x12", "spleen-8x16", "spleen-12x24"];

impl Default for TextPaint {
    fn default() -> Self {
        TextPaint {
            keys: vec!["name".into()],
            upper: false,
            font: "spleen-6x12".into(),
            marker: None,
            spacing: 60,
            padding: 1,
            ink: Ink::Black,
            color: None,
            background: None,
        }
    }
}

impl TextPaint {
    fn validate(&self) -> Result<(), String> {
        if !FONTS.contains(&self.font.as_str()) {
            return Err(format!("text.font {:?}: one of {}", self.font, FONTS.join(", ")));
        }
        if self.keys.is_empty() {
            return Err("text.keys: at least one tag to read the name from".into());
        }
        Ok(())
    }
}

/// What a paint entry does with its geometry: decided by what it paints.
#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    /// An area.
    Fill(FillPaint),
    /// Lines.
    Line(LinePaint),
    /// Features, by their names.
    Text(TextPaint),
}

impl Look {
    pub fn key(&self) -> &'static str {
        match self {
            Look::Fill(_) => "fill",
            Look::Line(_) => "line",
            Look::Text(_) => "text",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Look::Fill(f) => f.validate(),
            Look::Line(l) => l.validate(),
            Look::Text(t) => t.validate(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Look::Fill(f) => serde_json::to_value(f),
            Look::Line(l) => serde_json::to_value(l),
            Look::Text(t) => serde_json::to_value(t),
        }
        .unwrap_or(serde_json::Value::Null)
    }
}

/// One `paint` entry: a layer of the screen, painted over the ones before.
#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    pub id: String,
    /// An area (`$areas.x`, a module's area), lines (`$lines.x`,
    /// `roads.centerlines`) or features (`$features.x`).
    pub from: Target,
    pub look: Look,
    pub halo: Halo,
    pub at: Option<At>,
}
