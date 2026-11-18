"""Point the workspace at crates.io for the sibling libraries.

Locally they are checkouts beside this one (`../penfill`, …); a CI runner has
only this repository, and the published versions are the same code. Drops
each sibling `path` and the `[patch.crates-io]` section from Cargo.toml.
"""

import re
from pathlib import Path

toml = Path(__file__).resolve().parent.parent / "Cargo.toml"
s = toml.read_text()
s = re.sub(r'path = "\.\./[^"]+", ', "", s)
cut = s.find("# The sibling libraries are developed alongside")
if cut != -1:
    s = s[:cut].rstrip() + "\n"
toml.write_text(s)
print(s[s.index("[workspace.dependencies]"):s.index("rstar")])
