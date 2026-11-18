# drawland for Python

OpenStreetMap drawn as layered pen-plotter SVGs. A short JavaScript
**profile** says what is drawn and how; `drawland` runs it at a place,
downloads what it reads (vector tiles, Overpass, elevation, fonts, datasets;
cached on disk) and returns one SVG in paper millimetres, a group per pen.

![City Grain](https://raw.githubusercontent.com/piLeoni/drawland/main/docs/gallery/city-grain.webp)

```sh
pip install drawland
```

```python
import drawland

# The built-in postcard, at a place.
m = drawland.draw(lat=37.8771, lng=-122.2562)
m.save("berkeley.svg")

# A profile of your own, or a style saved from https://draw.land
m = drawland.draw("city-grain.js", address="Piazza Maggiore, Bologna", progress=True)
print(m.title, [(g, n) for g, n in m.groups])   # a group is a pen, with its strokes
m.save("bologna.svg", per_layer=True)          # and one file per pen

# A script's text; what it reads beside it comes from dir.
m = drawland.draw(script=open("lake.js").read(), dir=".")

# The graph only: no download.
g = drawland.graph("city-grain.js", lat=45.07, lng=7.69)
```

The package installs the command line too: `drawland map city-grain.js
--lat 37.8771 --lng -122.2562 -o map.svg` (or `python -m drawland map …`).

| | |
|---|---|
| `draw(profile=None, *, script, dir, lat, lng, address, cache="cache", offline, refresh, progress)` | Draw; a `Map` |
| `graph(profile=None, *, script, dir, lat, lng, cache)` | The graph the profile builds, as a dict |
| `Map.svg`, `.title`, `.groups`, `.slices` | The whole SVG; the profile's title; `(id, strokes)` per pen; `{name: svg}` per sheet |
| `Map.save(path, *, per_layer, slices, decimals)` | Write the files; their paths |
| `Map.files(*, per_layer, slices, decimals)` | The same as `{suffix: svg}` |
| `POSTCARD` | The built-in profile's script |
| `cli(argv)` | The command line; its exit code |

Errors (a script mistake says `line N column M`, a download that failed) are
`RuntimeError`. `draw` releases the GIL while it works.

How to write a profile, every pattern and the full reference:
[draw.land/docs](https://draw.land/docs). Source:
[github.com/piLeoni/drawland](https://github.com/piLeoni/drawland).
AGPL-3.0-only.
