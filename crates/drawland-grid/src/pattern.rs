//! The 8 × 8 pixel patterns an area is filled with, and ordered dithering
//! for a grey. Both are read at a dot's place in the world, not on the
//! screen, so they hold still as the map moves under them.

/// A tile of 8 rows, the top first; in each row the leftmost pixel is the
/// highest bit.
#[derive(Clone, Copy, Debug)]
pub struct Pattern([u8; 8]);

impl Pattern {
    pub fn named(name: &str) -> Option<Pattern> {
        let rows = match name {
            "solid" => [0xFF; 8],
            "dots" => [0x88, 0x00, 0x22, 0x00, 0x88, 0x00, 0x22, 0x00],
            "sparse" => [0x80, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00],
            "grid" => [0xFF, 0x88, 0x88, 0x88, 0xFF, 0x88, 0x88, 0x88],
            "lines" => [0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00],
            "columns" => [0x88; 8],
            "hatch" => [0x11, 0x22, 0x44, 0x88, 0x11, 0x22, 0x44, 0x88],
            "backhatch" => [0x88, 0x44, 0x22, 0x11, 0x88, 0x44, 0x22, 0x11],
            "cross" => [0x99, 0x66, 0x66, 0x99, 0x99, 0x66, 0x66, 0x99],
            "checker" => [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            "bricks" => [0xFF, 0x80, 0x80, 0x80, 0xFF, 0x08, 0x08, 0x08],
            "waves" => [0x1C, 0x22, 0xC1, 0x00, 0x1C, 0x22, 0xC1, 0x00],
            "stipple" => [0x80, 0x04, 0x20, 0x01, 0x08, 0x40, 0x02, 0x10],
            "diamonds" => [0x10, 0x28, 0x44, 0x82, 0x44, 0x28, 0x10, 0x00],
            _ => return None,
        };
        Some(Pattern(rows))
    }

    /// Whether the pattern inks the dot at `(x, y)` of the world.
    pub fn at(&self, x: i64, y: i64) -> bool {
        let row = self.0[y.rem_euclid(8) as usize];
        row & (0x80 >> x.rem_euclid(8)) != 0
    }
}

/// The 8 × 8 Bayer matrix: thresholds 0 to 63, spread so that every grey
/// is an even scatter of dots.
const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Whether a grey `tone` (0 white, 1 black) inks the dot at `(x, y)`.
pub fn tone_at(tone: f64, x: i64, y: i64) -> bool {
    let t = BAYER[y.rem_euclid(8) as usize][x.rem_euclid(8) as usize] as f64;
    t + 0.5 < tone * 64.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_pattern_is_defined() {
        for name in drawland_core::paint::PATTERNS {
            assert!(Pattern::named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn a_tone_inks_its_share_of_dots() {
        for (tone, want) in [(0.0, 0), (0.25, 16), (0.5, 32), (1.0, 64)] {
            let n = (0..8).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|&(x, y)| tone_at(tone, x, y)).count();
            assert_eq!(n, want, "tone {tone}");
        }
    }

    #[test]
    fn patterns_tile_the_world() {
        let p = Pattern::named("hatch").unwrap();
        assert_eq!(p.at(3, 0), p.at(-5, 8));
    }
}
