"""draw.land: OpenStreetMap drawn as layered pen-plotter SVGs.

Draw a profile, a JavaScript or TypeScript file (every style in the gallery at
https://draw.land is one):

    import drawland
    m = drawland.draw("city-grain.js", lat=37.8053, lng=-122.4436)
    m.save("map.svg")

or build the graph here, in Python, with the same library:

    g = drawland.Graph(lat=37.802, lng=-122.263)
    g.output(width="6in", height="4in", scale="1:8000")
    water = g.areas.polygons(g.features.tiles("water"))
    g.draw(id="water", from_=g.fills.fill(in_=water, pattern="waves", spacing="0.8mm"))
    drawland.draw(g).save("lake.svg")

Options are keyword arguments with the library's own names (``minLength``,
``exteriorOnly``: as in drawland.d.ts and the reference at
https://draw.land/docs); ``in_`` and ``from_`` are ``in`` and ``from``.
"""

import json as _json

from ._drawland import (
    POSTCARD,
    Error,
    Handle,
    Map,
    __version__,
    _Graph,
    cli,
    draw as _draw,
    functions as _functions,
    graph as _graph,
    properties as _properties,
)

__all__ = ["draw", "graph", "Graph", "Map", "Handle", "Error", "POSTCARD", "FUNCTIONS", "cli", "__version__"]

#: Every function of the library, as a script names it (``areas.union``).
FUNCTIONS = tuple(_functions())
_PROPERTIES = frozenset(_properties())
_NAMES = frozenset(FUNCTIONS)


def _key(k):
    """``in_`` → ``in``: Python's keywords, written with an underscore."""
    return k[:-1] if len(k) > 1 and k.endswith("_") else k


def _camel(name):
    """``fill_holes`` → ``fillHoles``: the library's names, the Python way."""
    head, *rest = name.split("_")
    return head + "".join(p[:1].upper() + p[1:] for p in rest)


def _options(args, kwargs):
    """Positional arguments, and the keyword ones as the options object
    that ends them."""
    args = list(args)
    if kwargs:
        opts = {_key(k): v for k, v in kwargs.items() if v is not None}
        if args and isinstance(args[-1], dict):
            args[-1] = {**args[-1], **opts}
        else:
            args.append(opts)
    return args


class Part:
    """A module's parts, made when first read: ``labels.area``,
    ``graticule.keys``, a label rule's ``lines``."""

    def __init__(self, graph, handle):
        self._graph = graph
        self._handle = handle
        self._made = {}

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        parts = self._handle.parts
        part = name if name in parts else _camel(name)
        if part not in parts:
            raise AttributeError(f"no part {name}; this has {', '.join(parts)}")
        if part not in self._made:
            self._made[part] = self._graph._wrap(self._graph._g.get(self._handle, part))
        return self._made[part]

    def __dir__(self):
        return list(self._handle.parts)

    def __repr__(self):
        return f"<drawland part: {', '.join(self._handle.parts)}>"


class _Space:
    """``g.areas``, ``g.features``…: the library's functions in one namespace."""

    def __init__(self, graph, space):
        self._graph = graph
        self._space = space

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        for candidate in (name, _camel(name)):
            full = f"{self._space}.{candidate}"
            if full in _PROPERTIES:
                return self._graph._prop(full)
            if full in _NAMES:
                return lambda *args, **kwargs: self._graph._call(full, args, kwargs)
        raise AttributeError(f"{self._space}.{name} is not in the library")

    def __dir__(self):
        prefix = self._space + "."
        return [n[len(prefix):] for n in (*_NAMES, *_PROPERTIES) if n.startswith(prefix)]


class Graph:
    """A draw.land graph, built in Python: the same library a JavaScript
    profile calls (``features``, ``areas``, ``lines``, ``fills``, ``modules``,
    ``compose``, ``ink``, ``burin``, ``page``, ``world``, ``engine``, and
    ``output``, ``title``, ``draw``, ``view``). Draw it with
    ``drawland.draw(g)``.

    ``lat`` and ``lng`` are the place ``world`` answers for (over the one
    ``output`` gives); ``dir`` is where views read the styles they name.
    """

    def __init__(self, *, lat=None, lng=None, dir="."):
        self._g = _Graph(lat=lat, lng=lng, dir=str(dir))
        for space in {n.split(".")[0] for n in (*_NAMES, *_PROPERTIES) if "." in n}:
            setattr(self, space, _Space(self, space))

    def _wrap(self, v):
        if isinstance(v, Handle) and v.kind == "part":
            return Part(self, v)
        if isinstance(v, dict):
            return {k: self._wrap(x) for k, x in v.items()}
        if isinstance(v, list):
            return [self._wrap(x) for x in v]
        return v

    def _call(self, name, args, kwargs):
        return self._wrap(self._g.call(name, _options(args, kwargs)))

    def _prop(self, name):
        return self._wrap(self._g.prop(name))

    def output(self, spec=None, **kwargs):
        """Where the map is and on what paper. A ``remap`` warp is a Python
        function from a page point ``[x, y]`` (mm) to where it goes; inside
        it, ``g.page.toWorld(p)`` reads the ground under a point."""
        spec = _options([spec] if spec is not None else [], kwargs)
        spec = spec[0] if spec else {}
        if not self._g.output_begin(spec):
            return
        warp = spec.get("warp")
        warps = [] if warp is None else warp if isinstance(warp, (list, tuple)) else [warp]
        done = []
        for n, w in enumerate(warps):
            if isinstance(w, dict) and "remap" in w:
                self._g.output_warps(done)
                columns, rows, points = self._g.remap_grid(w, n)
                went = [list(w["remap"](list(p))) for p in points]
                done.append(self._g.remap_mesh(n, columns, rows, points, went))
            else:
                done.append(w)
        self._g.output_warps(done)

    def title(self, text):
        """The map's title, for the SVG and the files."""
        self._call("title", [text], {})

    def draw(self, spec=None, **kwargs):
        """Draw lines as a group, a pen: ``draw(id="roads", from_=roads.outlines,
        stroke={...})``. Its group id."""
        return self._call("draw", [spec] if spec is not None else [], kwargs)

    def view(self, *args, **kwargs):
        """Another place drawn into a rectangle of the page:
        ``view(style, id="inset", at=[110, 8], size=[36, 24], lat=…, lng=…, scale="1:40000")``,
        where ``style`` is a function of the graph that draws it, or the name
        of a JavaScript style beside ``dir`` (``"city-grain"`` reads
        ``city-grain.js``). Its id."""
        style = next((a for a in args if callable(a) or isinstance(a, str)), None)
        specs = [a for a in args if isinstance(a, dict)]
        spec = _options(specs[:1], kwargs)
        spec = spec[0] if spec else {}
        if isinstance(style, str):
            return self._g.view_style(spec, style)
        if not callable(style):
            raise Error('view: the style is a name ("city-grain") or a function of the graph that draws')
        self._g.view_begin(spec)
        ok = False
        try:
            style(self)
            ok = True
        finally:
            ended = self._g.view_end(ok)
        return ended

    def to_dict(self):
        """The graph built, as the engine reads it. The graph is finished."""
        return _json.loads(self._g.finish())

    def __dir__(self):
        return sorted({*super().__dir__(), *(n.split(".")[0] for n in _NAMES)})


def draw(profile=None, **kwargs):
    """Draw a profile at a place: a ``.js``, ``.ts`` or ``.json`` file, a
    ``Graph`` built here, ``script=`` a script's text (``typescript=True``
    for TypeScript), or none: the built-in postcard. A ``Map``."""
    return _draw(profile, **kwargs)


def graph(profile=None, **kwargs):
    """The graph a profile builds, as a dict; no map data is downloaded."""
    return _json.loads(_graph(profile, **kwargs))
