# drawland for Node

OpenStreetMap drawn as layered pen-plotter SVGs. A short JavaScript
**profile** says what is drawn and how; `drawland` runs it at a place,
downloads what it reads (vector tiles, Overpass, elevation, fonts, datasets;
cached on disk) and returns one SVG in paper millimetres, a group per pen.

![City Grain](https://raw.githubusercontent.com/piLeoni/drawland/main/docs/gallery/city-grain.webp)

```sh
npm install drawland
```

```js
const drawland = require('drawland')   // or: import drawland from 'drawland'

// The built-in postcard, at a place.
const map = await drawland.draw({ lat: 37.8771, lng: -122.2562 })
map.save('berkeley.svg')

// A profile of your own, or a style saved from https://draw.land
const bologna = await drawland.draw({ profile: 'city-grain.js', address: 'Piazza Maggiore, Bologna', progress: true })
console.log(bologna.title, bologna.groups)        // [{ id, strokes }]: a group is a pen
bologna.save('bologna.svg', { perLayer: true })   // and one file per pen

// A script's text; what it reads beside it comes from dir.
await drawland.draw({ script: 'output({ lat: 45.07, lng: 7.69 })\n…', dir: '.' })

// The graph only: no download.
const graph = drawland.graph({ profile: 'city-grain.js', lat: 45.07, lng: 7.69 })
```

The package has the command line too: `npx drawland map city-grain.js
--lat 37.8771 --lng -122.2562 -o map.svg`.

| | |
|---|---|
| `draw({ profile, script, dir, lat, lng, address, cache, offline, refresh, progress })` | Draw, off the main thread: a `Promise<Map>` |
| `graph({ … })` | The graph the profile builds, as an object |
| `map.svg`, `.title`, `.groups`, `.slices` | The whole SVG; the profile's title; `{ id, strokes }` per pen; `{ suffix, svg }` per sheet |
| `map.save(path, { perLayer, slices, decimals })` | Write the files; their paths |
| `map.files({ … })` | The same as `{ suffix, svg }` |
| `POSTCARD` | The built-in profile's script |
| `cli(argv)` | The command line; its exit code |

Prebuilt for macOS (arm64, x64), Linux (x64, arm64, glibc) and Windows (x64).

How to write a profile, every pattern and the full reference:
[draw.land/docs](https://draw.land/docs). Source:
[github.com/piLeoni/drawland](https://github.com/piLeoni/drawland).
AGPL-3.0-only.
