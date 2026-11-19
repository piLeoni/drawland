//! A screen painted from layers made by hand: what each look puts on the
//! grid, the halos, and the patterns held to the world.

use drawland_core::grammar_run::{PaintShape, Painted};
use drawland_core::labels::{Named, Shape};
use drawland_core::paint::*;
use drawland_grid::{render, Format, Picture};
use geo::{Coord, LineString, MultiPolygon, Polygon};

fn screen(kind: ScreenKind, width: u32, height: u32) -> Screen {
    Screen { kind, width, height, invert: false }
}

/// The whole screen as an area, in mm.
fn page(s: &Screen) -> MultiPolygon<f64> {
    let (w, h) = s.size_mm();
    MultiPolygon::new(vec![Polygon::new(LineString::from(vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h), (0.0, 0.0)]), vec![])])
}

fn layer(id: &str, look: Look, shape: PaintShape, halo: u32) -> Painted {
    Painted { id: id.into(), look, halo: Halo(halo, halo), shape }
}

fn text(p: &Picture) -> String {
    String::from_utf8(p.encode(Format::Txt, "t").unwrap()).unwrap()
}

#[test]
fn a_text_screen_tiles_lines_and_names() {
    let s = screen(ScreenKind::Text, 24, 6);
    let (cw, ch) = s.unit_mm();
    let row = |r: f64| (r + 0.5) * ch;
    let road = LineString::from(vec![(0.0, row(3.0)), (24.0 * cw, row(3.0))]);
    let water = FillPaint { chars: Some(Tile(vec!["~.".chars().collect()])), ..FillPaint::default() };
    let name = Named { text: "Via Roma".into(), rank: 0.5, extent: 1.0, shape: Shape::Pin(Coord { x: 12.0 * cw, y: row(1.0) }), feature: 0 };
    let layers = vec![
        layer("water", Look::Fill(water), PaintShape::Area(page(&s)), 0),
        layer("road", Look::Line(LinePaint { style: LineStyle::Box, ..LinePaint::default() }), PaintShape::Lines(vec![road]), 0),
        layer("names", Look::Text(TextPaint { padding: 0, ..TextPaint::default() }), PaintShape::Names(vec![name]), 1),
    ];
    let out = text(&render(&s, (0, 0), &layers));
    let rows: Vec<&str> = out.lines().collect();
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[3], "─".repeat(24), "the road runs the width in box drawing");
    assert!(rows[5].starts_with("~.~.~."), "the water is tiled");
    assert!(rows[1].contains(" Via Roma "), "the name, a cell clear each side: {:?}", rows[1]);
    assert!(rows[0].contains(&" ".repeat(10)), "its halo clears the row above too");
}

#[test]
fn box_lines_that_cross_are_joined() {
    let s = screen(ScreenKind::Text, 9, 5);
    let (cw, ch) = s.unit_mm();
    let across = LineString::from(vec![(0.0, 2.5 * ch), (9.0 * cw, 2.5 * ch)]);
    let down = LineString::from(vec![(4.5 * cw, 0.0), (4.5 * cw, 5.0 * ch)]);
    let box_line = Look::Line(LinePaint { style: LineStyle::Box, ..LinePaint::default() });
    let layers = vec![layer("a", box_line.clone(), PaintShape::Lines(vec![across]), 0), layer("b", box_line, PaintShape::Lines(vec![down]), 0)];
    let out = text(&render(&s, (0, 0), &layers));
    assert_eq!(out.lines().nth(2).unwrap(), "────┼────");
    assert_eq!(out.lines().next().unwrap(), "    │");
}

#[test]
fn a_bitmap_halo_is_the_other_ink() {
    let s = screen(ScreenKind::Bitmap, 32, 16);
    let px = PIXEL_MM;
    let line = LineString::from(vec![(0.0, 8.5 * px), (32.0 * px, 8.5 * px)]);
    let layers = vec![
        layer("ground", Look::Fill(FillPaint { pattern: Some("solid".into()), ..FillPaint::default() }), PaintShape::Area(page(&s)), 0),
        layer("road", Look::Line(LinePaint::default()), PaintShape::Lines(vec![line]), 2),
    ];
    let Picture::Bitmap(b) = render(&s, (0, 0), &layers) else { panic!("a bitmap") };
    assert!(b.black(5, 8), "the line is black");
    assert!(!b.black(5, 7) && !b.black(5, 10), "two white pixels each side");
    assert!(b.black(5, 5) && b.black(5, 11), "and black past them");
    let png = b.png().unwrap();
    assert_eq!(&png[1..4], b"PNG");
}

#[test]
fn patterns_stay_on_the_ground_as_the_screen_moves() {
    let s = screen(ScreenKind::Bitmap, 24, 8);
    let tone = vec![layer("t", Look::Fill(FillPaint { tone: Some(0.3), ..FillPaint::default() }), PaintShape::Area(page(&s)), 0)];
    let Picture::Bitmap(a) = render(&s, (0, 0), &tone) else { panic!() };
    let Picture::Bitmap(b) = render(&s, (3, 0), &tone) else { panic!() };
    // The screen moved three pixels east over the world: what was at x + 3 is now at x.
    for y in 0..8 {
        for x in 0..21 {
            assert_eq!(a.black(x + 3, y), b.black(x, y));
        }
    }
}

#[test]
fn a_bitmap_shows_in_a_terminal_in_braille() {
    let s = screen(ScreenKind::Bitmap, 4, 4);
    let layers = vec![layer("all", Look::Fill(FillPaint { pattern: Some("solid".into()), ..FillPaint::default() }), PaintShape::Area(page(&s)), 0)];
    let out = text(&render(&s, (0, 0), &layers));
    assert_eq!(out, "⣿⣿\n");
}
