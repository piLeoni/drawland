// draw.land for Node: a profile and a place in, the layered pen-plotter SVG
// out, downloading and caching what the map reads.
//
//   const drawland = require('drawland')
//   const map = await drawland.draw({ profile: 'city-grain.js', lat: 37.8053, lng: -122.4436 })
//   map.save('map.svg')
//
// or a graph built here, with the same library a profile calls:
//
//   const g = new drawland.Graph()
//   g.output({ lat: 37.802, lng: -122.263, width: '6in', height: '4in', scale: '1:8000' })
//   const water = g.areas.polygons(g.features.tiles('water'))
//   g.draw({ id: 'water', from: g.fills.fill({ in: water, pattern: 'waves', spacing: '0.8mm' }) })
//   ;(await drawland.draw({ profile: g })).save('lake.svg')

const native = require('./native.js')

// The library's JavaScript face, the very file a profile's interpreter runs:
// a function of the library's calls, here a NativeGraph's.
const makeLibrary = new Function(`return (\n${native.LIBRARY}\n)`)()

const J = (v) => JSON.stringify(v === undefined ? null : v)
const P = (s) => JSON.parse(s)

class Graph {
  /** A graph built in Node. `lat`/`lng`: the place `world` answers for; `dir`: where views read the styles they name. */
  constructor({ lat, lng, dir } = {}) {
    const n = new native.NativeGraph(lat, lng, dir)
    const dl = {
      functions: () => native.functions(),
      properties: () => native.properties(),
      call: (name, args) => P(n.call(name, J(args))),
      prop: (name) => P(n.prop(name)),
      get: (of, part) => P(n.get(J(of), part)),
      describe: (v) => n.describe(v === undefined ? '' : J(v)),
      outputBegin: (spec) => n.outputBegin(J(spec)),
      outputWarps: (done) => n.outputWarps(J(done)),
      remapGrid: (warp, k) => P(n.remapGrid(J(warp), k)),
      remapMesh: (k, columns, rows, points, went) => P(n.remapMesh(k, columns, rows, J(points), J(went))),
      viewBegin: (spec) => n.viewBegin(J(spec)),
      viewEnd: (ok) => P(n.viewEnd(ok)),
      viewStyle: (spec, name) => P(n.viewStyle(J(spec), name)),
      burin: () => {
        throw new Error("burin.terrain((b) => …) is burin's script builder, which runs in a profile; from Node, pass the burin document's JSON")
      },
      fail: (message) => {
        throw new Error(message)
      },
    }
    Object.defineProperty(this, 'native', { value: n })
    Object.assign(this, makeLibrary(dl))
  }

  /** The graph built, as the engine reads it. The graph is finished. */
  finish() {
    return P(this.native.finish())
  }
}

/** Draw a profile at a place: a file (`profile`), a `Graph`, a script's text (`script`), or the built-in postcard. */
function draw(options = {}) {
  if (options.profile instanceof Graph) {
    const { profile, ...rest } = options
    return native.draw({ ...rest, graph: profile.native.finish() })
  }
  return native.draw(options)
}

/** The graph a profile builds, as an object; no map data is downloaded. */
function graph(options = {}) {
  if (options.profile instanceof Graph) return options.profile.finish()
  return P(native.graph(options))
}

module.exports = {
  draw,
  graph,
  Graph,
  cli: native.cli,
  Map: native.Map,
  POSTCARD: native.POSTCARD,
  FUNCTIONS: Object.freeze(native.functions()),
}
