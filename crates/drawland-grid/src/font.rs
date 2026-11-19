//! Pixel fonts: Spleen (Frederic Cambus, BSD 2-clause, `fonts/LICENSE-spleen`)
//! in four sizes, read from their PSF2 files. A character the font lacks is
//! written as its nearest Latin letters (`ő` → `o`), else `?`.

use std::collections::HashMap;
use std::sync::OnceLock;

/// A monospace bitmap font: every glyph `width` × `height` pixels.
pub struct Font {
    pub width: usize,
    pub height: usize,
    /// Bytes per glyph row.
    stride: usize,
    glyphs: Vec<u8>,
    /// Character → glyph index.
    map: HashMap<char, usize>,
}

const FILES: &[(&str, &[u8])] = &[
    ("spleen-5x8", include_bytes!("../fonts/spleen-5x8.psfu")),
    ("spleen-6x12", include_bytes!("../fonts/spleen-6x12.psfu")),
    ("spleen-8x16", include_bytes!("../fonts/spleen-8x16.psfu")),
    ("spleen-12x24", include_bytes!("../fonts/spleen-12x24.psfu")),
];

/// A built-in font by name (`drawland_core::paint::FONTS`), read once.
pub fn font(name: &str) -> Option<&'static Font> {
    static FONTS: OnceLock<Vec<(&'static str, Font)>> = OnceLock::new();
    let all = FONTS.get_or_init(|| {
        FILES.iter().map(|(n, bytes)| (*n, Font::psf(bytes).unwrap_or_else(|e| panic!("built-in font {n}: {e}")))).collect()
    });
    all.iter().find(|(n, _)| *n == name).map(|(_, f)| f)
}

fn u32_at(b: &[u8], i: usize) -> Result<usize, String> {
    b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize).ok_or_else(|| "truncated header".to_string())
}

impl Font {
    /// A PSF font (version 1 or 2) with its Unicode table.
    pub fn psf(b: &[u8]) -> Result<Font, String> {
        match b.get(0..4) {
            Some([0x72, 0xb5, 0x4a, 0x86]) => Font::psf2(b),
            Some([0x36, 0x04, ..]) => Font::psf1(b),
            _ => Err("not a PSF font".into()),
        }
    }

    /// PSF1: 8 pixels across, 256 or 512 glyphs, the table in UCS-2.
    fn psf1(b: &[u8]) -> Result<Font, String> {
        let mode = *b.get(2).ok_or("truncated header")?;
        let height = *b.get(3).ok_or("truncated header")? as usize;
        let count = if mode & 1 == 1 { 512 } else { 256 };
        let end = 4 + count * height;
        let glyphs = b.get(4..end).ok_or("truncated glyphs")?.to_vec();
        let mut map = HashMap::new();
        if mode & 2 == 2 {
            let mut glyph = 0usize;
            let mut in_sequence = false;
            for pair in b[end..].chunks_exact(2) {
                match u16::from_le_bytes([pair[0], pair[1]]) {
                    0xFFFF => {
                        glyph += 1;
                        in_sequence = false;
                    }
                    0xFFFE => in_sequence = true,
                    u if !in_sequence => {
                        if let Some(c) = char::from_u32(u as u32) {
                            map.entry(c).or_insert(glyph);
                        }
                    }
                    _ => {}
                }
                if glyph >= count {
                    break;
                }
            }
        } else {
            for g in 0..count.min(256) {
                map.insert(char::from_u32(g as u32).unwrap_or('?'), g);
            }
        }
        Ok(Font { width: 8, height, stride: 1, glyphs, map })
    }

    /// PSF2: any size, the table in UTF-8.
    fn psf2(b: &[u8]) -> Result<Font, String> {
        let header = u32_at(b, 8)?;
        let flags = u32_at(b, 12)?;
        let count = u32_at(b, 16)?;
        let size = u32_at(b, 20)?;
        let height = u32_at(b, 24)?;
        let width = u32_at(b, 28)?;
        let stride = width.div_ceil(8);
        if size != stride * height {
            return Err(format!("glyph size {size} is not {stride} × {height}"));
        }
        let end = header + count * size;
        let glyphs = b.get(header..end).ok_or("truncated glyphs")?.to_vec();
        let mut map = HashMap::new();
        if flags & 1 == 1 {
            // Per glyph: UTF-8 characters, then sequences after 0xFE, ending 0xFF.
            let mut glyph = 0usize;
            let mut i = end;
            let mut in_sequence = false;
            let mut start = i;
            while i < b.len() && glyph < count {
                match b[i] {
                    0xFF => {
                        if !in_sequence {
                            add(&mut map, &b[start..i], glyph);
                        }
                        glyph += 1;
                        in_sequence = false;
                        i += 1;
                        start = i;
                    }
                    0xFE => {
                        if !in_sequence {
                            add(&mut map, &b[start..i], glyph);
                        }
                        in_sequence = true;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
        } else {
            for g in 0..count.min(256) {
                if let Some(c) = char::from_u32(g as u32) {
                    map.insert(c, g);
                }
            }
        }
        Ok(Font { width, height, stride, glyphs, map })
    }

    /// The glyph for `c`, or its stand-in: a letter the font lists but
    /// leaves blank (the smallest Spleen has no accents) is a missing one.
    fn index(&self, c: char) -> usize {
        if let Some(&g) = self.map.get(&c) {
            if c.is_whitespace() || !self.blank(g) {
                return g;
            }
        }
        let near = deunicode::deunicode_char(c).and_then(|s| s.chars().next());
        near.and_then(|n| self.map.get(&n).copied()).or_else(|| self.map.get(&'?').copied()).unwrap_or(0)
    }

    fn blank(&self, g: usize) -> bool {
        let size = self.stride * self.height;
        self.glyphs[g * size..(g + 1) * size].iter().all(|b| *b == 0)
    }

    /// Whether the font has a glyph of its own for `c`.
    pub fn has(&self, c: char) -> bool {
        self.map.get(&c).is_some_and(|&g| c.is_whitespace() || !self.blank(g))
    }

    /// Whether the glyph of `c` inks its pixel `(x, y)`.
    pub fn ink(&self, c: char, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let g = self.index(c);
        let row = self.glyphs[g * self.stride * self.height + y * self.stride + x / 8];
        row & (0x80 >> (x % 8)) != 0
    }
}

/// The characters of one glyph's entry in the table: all but sequences.
fn add(map: &mut HashMap<char, usize>, utf8: &[u8], glyph: usize) {
    if let Ok(s) = std::str::from_utf8(utf8) {
        for c in s.chars() {
            map.entry(c).or_insert(glyph);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_font_reads_and_has_latin_letters() {
        for name in drawland_core::paint::FONTS {
            let f = font(name).unwrap_or_else(|| panic!("{name}"));
            let inked = |c| (0..f.height).flat_map(|y| (0..f.width).map(move |x| (x, y))).filter(|&(x, y)| f.ink(c, x, y)).count();
            assert!(inked('A') > 4, "{name} A");
            assert_eq!(inked(' '), 0, "{name} space");
            assert!(inked('é') >= inked('e'), "{name} é is written");
        }
        let f = font("spleen-6x12").unwrap();
        assert_eq!((f.width, f.height), (6, 12));
    }

    #[test]
    fn a_missing_letter_falls_back_to_its_latin_one() {
        let f = font("spleen-5x8").unwrap();
        let rows = |c| (0..f.height).map(|y| (0..f.width).map(|x| f.ink(c, x, y)).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(rows('ǅ'), rows('D'));
    }
}
