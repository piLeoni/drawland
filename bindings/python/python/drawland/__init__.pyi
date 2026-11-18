from os import PathLike
from pathlib import Path
from typing import Any

__version__: str
POSTCARD: str

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
    profile: str | PathLike[str] | None = None,
    *,
    script: str | None = None,
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
    profile: str | PathLike[str] | None = None,
    *,
    script: str | None = None,
    dir: str | PathLike[str] | None = None,
    lat: float | None = None,
    lng: float | None = None,
    cache: str | PathLike[str] = "cache",
) -> dict[str, Any]: ...
def cli(argv: list[str]) -> int: ...
