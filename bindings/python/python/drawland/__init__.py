"""draw.land: OpenStreetMap drawn as layered pen-plotter SVGs.

    import drawland
    m = drawland.draw("city-grain.js", lat=37.8771, lng=-122.2562)
    m.save("map.svg")

A profile is a short JavaScript file; the styles in the gallery at
https://draw.land/docs are profiles. With no profile, the built-in postcard.
"""

from ._drawland import POSTCARD, Map, __version__, cli, draw, graph as _graph
import json as _json

__all__ = ["draw", "graph", "Map", "POSTCARD", "cli", "__version__"]


def graph(profile=None, *, script=None, dir=None, lat=None, lng=None, cache="cache"):
    """The graph a profile builds, as a dict; no map data is downloaded."""
    return _json.loads(_graph(profile, script=script, dir=dir, lat=lat, lng=lng, cache=cache))
