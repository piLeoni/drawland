// 1-bit night: the day's plan with the lights out, drawn as the day is and
// written inverted (`invert`): the ground black, buildings in a fine grain
// of lit windows, streets and shores in white lines, the sea in white
// waves, main roads dark between light edges, names white. The day is
// `1-bit.js`.
// `drawland map configs/1-bit-night.js -o map.png`.
title("1-bit night")

// ---------------------------------------------------------------------------
// 1. PARAMETERS & STYLE PALETTE
// ---------------------------------------------------------------------------

output({
  lat: 45.4375,
  lng: 12.3358,
  scale: "3m", // the ground one pixel spans
  projection: "webmercator",
  screen: { kind: "bitmap", width: 800, height: 480, invert: true },
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
// Inverted, a dot of ink is a lit window.
paint({ id: "buildings", from: buildings, fill: { pattern: "dots" } })
paint({ id: "streets", from: minor, halo: 1 })
paint({ id: "rail", from: rail, line: { dash: [4, 2] }, halo: 1 })
// Main roads white between black edges: a black line two pixels wider,
// then the road over it in white.
paint({ id: "road_edges", from: major, line: { width: 5 }, halo: 1 })
paint({ id: "roads", from: major, line: { width: 3, ink: "white" } })

// The names in a hierarchy, by the zoom and what is named (Mapbox
// expressions, as a web map's style writes them): from far, the oceans
// largest, then the seas, the capitals small with a dot; nearer, a city's
// name grows past its quarters', which come in only near, smallest.
paint({ id: "street_names", from: streetNames, minzoom: 14, filter: ["!=", ["get", "class"], "ferry"], text: { font: "spleen-5x8", spacing: 220 }, halo: 1 })
paint({
  id: "water_names",
  from: features.tiles("water_name:*"),
  filter: ["any", ["match", ["get", "class"], ["ocean", "sea"], true, false], [">=", ["zoom"], 9]],
  text: {
    font: ["match", ["get", "class"],
      "ocean", ["step", ["zoom"], "spleen-8x16", 6, "spleen-6x12"],
      "sea", "spleen-6x12",
      ["step", ["zoom"], "spleen-5x8", 14, "spleen-6x12"]],
  },
  halo: 1,
})
paint({
  id: "places",
  from: features.tiles("place:city,town,village,suburb,quarter,neighbourhood,island"),
  // Capitals from the globe on, cities from a continent's width, then
  // towns, villages and suburbs, quarters last.
  filter: ["case",
    ["==", ["get", "capital"], 2], true,
    ["==", ["get", "class"], "city"], [">=", ["zoom"], 4],
    ["==", ["get", "class"], "town"], [">=", ["zoom"], 8],
    ["match", ["get", "class"], ["village", "suburb", "island"], true, false], [">=", ["zoom"], 11],
    [">=", ["zoom"], 14]],
  text: {
    // A country's capital a step ahead of its cities.
    font: ["match", ["get", "class"],
      "city", ["case",
        ["==", ["get", "capital"], 2], ["step", ["zoom"], "spleen-6x12", 6, "spleen-8x16", 10, "spleen-12x24"],
        ["step", ["zoom"], "spleen-5x8", 6, "spleen-6x12", 9, "spleen-8x16", 12, "spleen-12x24"]],
      "town", ["step", ["zoom"], "spleen-5x8", 11, "spleen-6x12", 13, "spleen-8x16"],
      ["suburb", "village", "island"], ["step", ["zoom"], "spleen-5x8", 14, "spleen-6x12", 16, "spleen-8x16"],
      ["step", ["zoom"], "spleen-5x8", 16, "spleen-6x12"]],
    upper: ["match", ["get", "class"], ["city", "town", "suburb"], true, false],
    marker: ["match", ["get", "class"], "city", 5, "town", 4, "village", 3, null],
    padding: 2,
  },
  halo: 2,
})
