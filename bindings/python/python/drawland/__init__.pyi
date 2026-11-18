from os import PathLike
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence

__version__: str
POSTCARD: str
#: Every function of the library, as a script names it (``areas.union``).
FUNCTIONS: tuple[str, ...]

class Error(RuntimeError):
    """A mistake in a profile, a download that failed, or no place given."""

class Handle:
    """Features, an area or lines, as the library returns them."""
    @property
    def kind(self) -> str: ...
    @property
    def reference(self) -> str: ...
    @property
    def parts(self) -> list[str]: ...

class Part:
    """A module's parts, made when first read: ``labels.area``, ``graticule.keys``, a rule's ``lines``."""
    def __getattr__(self, name: str) -> Any: ...

class _Space:
    """One of the library's namespaces: ``g.areas``, ``g.features``… Its
    functions take the library's arguments, positional, and its options as
    keywords (``in_``, ``from_`` for ``in``, ``from``)."""
    def __getattr__(self, name: str) -> Any: ...

class Graph:
    """A draw.land graph built in Python, with the library a JavaScript profile calls."""
    features: _Space
    areas: _Space
    lines: _Space
    fills: _Space
    modules: _Space
    compose: _Space
    ink: _Space
    burin: _Space
    engine: _Space
    page: _Space
    world: _Space
    def __init__(self, *, lat: float | None = None, lng: float | None = None, dir: str | PathLike[str] = ".") -> None: ...
    def output(self, spec: Mapping[str, Any] | None = None, **options: Any) -> None: ...
    def title(self, text: str) -> None: ...
    def draw(self, spec: Mapping[str, Any] | None = None, **options: Any) -> str: ...
    def view(self, *style_and_spec: Callable[["Graph"], None] | str | Mapping[str, Any], **spec: Any) -> str: ...
    def to_dict(self) -> dict[str, Any]: ...

class Map:
    @property
    def title(self) -> str | None: ...
    @property
    def svg(self) -> str: ...
    @property
    def groups(self) -> list[tuple[str, int]]: ...
    @property
    def slices(self) -> dict[str, str]: ...
    def files(self, *, per_layer: bool = False, slices: str = "both", decimals: int | None = None) -> dict[str, str]: ...
    def save(self, path: str | PathLike[str], *, per_layer: bool = False, slices: str = "both", decimals: int | None = None) -> list[Path]: ...

def draw(
    profile: str | PathLike[str] | Graph | None = None,
    *,
    script: str | None = None,
    typescript: bool = False,
    dir: str | PathLike[str] | None = None,
    lat: float | None = None,
    lng: float | None = None,
    address: str | None = None,
    cache: str | PathLike[str] = "cache",
    offline: bool = False,
    refresh: bool = False,
    progress: bool = False,
) -> Map: ...
def graph(
    profile: str | PathLike[str] | Graph | None = None,
    *,
    script: str | None = None,
    typescript: bool = False,
    dir: str | PathLike[str] | None = None,
    lat: float | None = None,
    lng: float | None = None,
    cache: str | PathLike[str] = "cache",
) -> dict[str, Any]: ...
def cli(argv: Sequence[str]) -> int: ...
