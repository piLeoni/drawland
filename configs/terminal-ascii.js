// Terminal ASCII: the map in plain ASCII on a green phosphor screen, as an
// old terminal would draw it. Water in tildes, land blank, buildings in
// hashes, roads in `- | / \ +`, names in capitals for places. No Braille,
// no box drawing: it reads on any terminal, in any font.
// `drawland view configs/terminal-ascii.js`.
title("Terminal ASCII")

// ---------------------------------------------------------------------------
// 1. PARAMETERS & STYLE PALETTE
// ---------------------------------------------------------------------------

output({
  lat: 45.4375,
  lng: 12.3358,
  scale: "8m", // the ground one character spans (a row, twice that)
  projection: "webmercator",
  screen: { kind: "text", width: 120, height: 44 },
})

const INK = {
  screen: "#0a0f0a",
  dim: "#1f7a2e",
  mid: "#2fbf47",
  bright: "#41ff41",
}

const MAJOR = ["motorway", "trunk", "primary", "secondary"]
const MINOR = ["tertiary", "minor", "service"]

// ---------------------------------------------------------------------------
// 2. FEATURES & GEOMETRY
// ---------------------------------------------------------------------------

const water = areas.polygons(features.tiles("water"))
const parks = areas.difference(areas.polygons(features.tiles("parks", "grass", "forest")), water)
const buildings = areas.polygons(features.tiles("buildings"))
const roads = features.tiles("roads")
const major = lines.of(features.where(roads, { class: MAJOR }))
const minor = lines.of(features.where(roads, { class: MINOR }))
const rail = lines.of(features.tiles("rail"))

// ---------------------------------------------------------------------------
// 3. PAINT, bottom first
// ---------------------------------------------------------------------------

paint({ id: "land", from: page.frame, fill: { background: INK.screen } })
paint({ id: "water", from: water, fill: { chars: ["~  ", "  ~"], color: INK.dim, background: INK.screen } })
paint({ id: "parks", from: parks, fill: { chars: ",", color: INK.dim } })
paint({ id: "buildings", from: buildings, fill: { chars: "#", color: INK.dim } })
paint({ id: "streets", from: minor, line: { style: "ascii", color: INK.mid } })
paint({ id: "rail", from: rail, line: { char: "=", color: INK.mid } })
paint({ id: "roads", from: major, line: { style: "ascii", color: INK.bright } })

paint({ id: "street_names", from: features.tiles("road_names"), text: { color: INK.mid, spacing: 50 } })
paint({ id: "water_names", from: features.tiles("water_name:*"), text: { color: INK.mid } })
paint({
  id: "places",
  from: features.tiles("place:city,town,village,suburb,quarter,neighbourhood,island"),
  text: { upper: true, marker: "*", color: INK.bright, padding: 2 },
  halo: [1, 0],
})
