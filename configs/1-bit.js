// 1-bit day: the map in black and white pixels, every pixel one or the other
// (800 × 480). Buildings solid black, like Nolli's plan of Rome, so the
// streets are the white between them; water in waves, parks stippled;
// main roads white between black edges; names in a pixel
// font, each with a pixel of clear screen round its letters. The night is
// `1-bit-night.js`.
// `drawland map configs/1-bit.js -o map.png`.
title("1-bit day")

// ---------------------------------------------------------------------------
// 1. PARAMETERS & STYLE PALETTE
// ---------------------------------------------------------------------------

output({
  lat: 45.4375,
  lng: 12.3358,
  scale: "3m", // the ground one pixel spans
  projection: "webmercator",
  screen: { kind: "bitmap", width: 800, height: 480 },
})

const MAJOR = ["motorway", "trunk", "primary", "secondary"]
const MINOR = ["tertiary", "minor", "service"]
// The streets named: the main ones and the next class down.
const NAMED = [...MAJOR, "tertiary"]

// ---------------------------------------------------------------------------
// 2. FEATURES & GEOMETRY
// ---------------------------------------------------------------------------

const water = areas.polygons(features.tiles("water"))
const canals = lines.of(features.tiles("waterway:canal,river,stream"))
// Parks on land only: a marine reserve (the Pelagos Sanctuary, a triangle
// from Provence to Liguria to Corsica) is a park in the tiles, not on the sea.
const parks = areas.difference(areas.polygons(features.tiles("parks", "grass", "forest")), water)
const buildings = areas.polygons(features.tiles("buildings"))
const roads = features.tiles("roads")
const major = lines.of(features.where(roads, { class: MAJOR }))
const minor = lines.of(features.where(roads, { class: MINOR }))
const rail = lines.of(features.tiles("rail"))
const streetNames = features.where(features.tiles("road_names"), { class: NAMED })

// ---------------------------------------------------------------------------
// 3. PAINT, bottom first
// ---------------------------------------------------------------------------

paint({ id: "water", from: water, fill: { pattern: "waves" } })
paint({ id: "shore", from: lines.rings(water) })
paint({ id: "canals", from: canals, halo: 1 })
paint({ id: "parks", from: parks, fill: { pattern: "stipple" } })
paint({ id: "buildings", from: buildings, fill: { pattern: "solid" } })
paint({ id: "streets", from: minor, halo: 1 })
paint({ id: "rail", from: rail, line: { dash: [4, 2] }, halo: 1 })
// Main roads white between black edges: a black line two pixels wider,
// then the road over it in white.
paint({ id: "road_edges", from: major, line: { width: 5 }, halo: 1 })
paint({ id: "roads", from: major, line: { width: 3, ink: "white" } })

paint({ id: "street_names", from: streetNames, text: { font: "spleen-5x8", spacing: 220 }, halo: 1 })
paint({ id: "water_names", from: features.tiles("water_name:*"), text: { font: "spleen-6x12" }, halo: 1 })
paint({
  id: "places",
  from: features.tiles("place:city,town,village,suburb,quarter,neighbourhood,island"),
  text: { font: "spleen-8x16", upper: true, marker: 5, padding: 2 },
  halo: 2,
})
