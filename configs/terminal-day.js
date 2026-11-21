// Terminal day: the night's map (`terminal.js`) in daylight, on paper:
// ink-blue waves on a pale sea, buildings in warm grey blocks, roads in
// ochre, names dark. Water in tilde waves, buildings in quarter blocks (the calli show between them),
// main roads in box drawing joined where they cross, the rest in Braille,
// the names a letter a cell. `drawland map configs/terminal-day.js -o -` prints
// it; `drawland view configs/terminal-day.js` moves round it.
title("Terminal day")

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
  land: "#f3efe6",
  sea: "#d4e3ee",
  wave: "#4f7fae",
  canal: "#3f78b5",
  park: "#dfead2",
  leaf: "#5a8f45",
  building: "#c9c1b2",
  street: "#8c8a84",
  road: "#b8741a",
  rail: "#8a5cb8",
  name: "#2a2a2a",
  place: "#a8361a",
  waterName: "#2b5f96",
}

/** @type {string[]} */
const WAVES = ["~     ", "   ~  "]
const MAJOR = ["motorway", "trunk", "primary", "secondary"]
const MINOR = ["tertiary", "minor", "service"]

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
const paths = lines.of(features.tiles("paths"))
const rail = lines.of(features.tiles("rail"))

// ---------------------------------------------------------------------------
// 3. PAINT, bottom first
// ---------------------------------------------------------------------------

paint({ id: "land", from: page.frame, fill: { background: INK.land } })
paint({ id: "water", from: water, fill: { chars: WAVES, color: INK.wave, background: INK.sea } })
paint({ id: "canals", from: canals, line: { color: INK.canal } })
paint({ id: "parks", from: parks, fill: { chars: "\"", color: INK.leaf, background: INK.park } })
paint({ id: "buildings", from: buildings, fill: { blocks: true, color: INK.building } })
paint({ id: "paths", from: paths, line: { color: INK.street } })
paint({ id: "streets", from: minor, line: { color: INK.street } })
paint({ id: "rail", from: rail, line: { style: "double", color: INK.rail } })
paint({ id: "roads", from: major, line: { style: "box", color: INK.road } })

// Names: places first (a mark, the name beside it), then water, then streets.
paint({ id: "street_names", from: features.tiles("road_names"), text: { color: INK.name, spacing: 50 } })
paint({ id: "water_names", from: features.tiles("water_name:*"), text: { color: INK.waterName } })
paint({
  id: "places",
  from: features.tiles("place:city,town,village,suburb,quarter,neighbourhood,island"),
  text: { upper: true, marker: "◆", color: INK.place, padding: 2 },
  halo: [1, 0],
})
