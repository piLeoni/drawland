// 1-bit chart: the map as a sea chart in black and white pixels (800 ×
// 480). The sea solid black and the land white, the coast a white rule
// round it; buildings drawn as their outlines, so the town reads as a fine
// grid; roads in black, names white on the water, black on the land.
// `drawland serve configs/1-bit.js` offers it beside 1-bit.
title("1-bit chart")

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

// ---------------------------------------------------------------------------
// 2. FEATURES & GEOMETRY
// ---------------------------------------------------------------------------

const water = areas.polygons(features.tiles("water"))
const canals = lines.of(features.tiles("waterway:canal,river,stream"))
const parks = areas.difference(areas.polygons(features.tiles("parks", "grass", "forest")), water)
const buildings = areas.polygons(features.tiles("buildings"))
const roads = features.tiles("roads")
const major = lines.of(features.where(roads, { class: MAJOR }))
const minor = lines.of(features.where(roads, { class: MINOR }))
const rail = lines.of(features.tiles("rail"))

// ---------------------------------------------------------------------------
// 3. PAINT, bottom first
// ---------------------------------------------------------------------------

paint({ id: "water", from: water, fill: { pattern: "solid" } })
paint({ id: "canals", from: canals, line: { width: 2 } })
paint({ id: "parks", from: parks, fill: { pattern: "sparse" } })
paint({ id: "buildings", from: lines.rings(buildings) })
paint({ id: "streets", from: minor, line: { dash: [1, 1] } })
paint({ id: "rail", from: rail, line: { dash: [4, 2] }, halo: 1 })
paint({ id: "roads", from: major, line: { width: 2 }, halo: 1 })

paint({ id: "water_names", from: features.tiles("water_name:*"), text: { font: "spleen-6x12", ink: "white" } })
paint({
  id: "places",
  from: features.tiles("place:city,town,village,suburb,quarter,neighbourhood,island"),
  text: { font: "spleen-8x16", upper: true, marker: 4, padding: 2 },
  halo: 2,
})
