// draw.land for Node: a profile and a place in, the layered pen-plotter SVG
// out, downloading and caching what the map reads.
//
//   const drawland = require('drawland')
//   const map = await drawland.draw({ profile: 'city-grain.js', lat: 37.8771, lng: -122.2562 })
//   map.save('map.svg')

const native = require('./native.js')

/** The graph a profile builds, as an object; no map data is downloaded. */
function graph(options) {
  return JSON.parse(native.graph(options))
}

module.exports = {
  draw: native.draw,
  graph,
  cli: native.cli,
  Map: native.Map,
  POSTCARD: native.POSTCARD,
}
