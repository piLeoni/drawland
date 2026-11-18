#!/usr/bin/env bash
# Publish a release from a green `Build` run: the crates to crates.io, the
# wheels to PyPI, the Node package (every platform's addon in one package)
# to npm. Needs `cargo login`, a PyPI token in ~/.pypirc and `npm login`.
#
#   scripts/publish.sh <run-id>      # gh run list --workflow build.yml
set -euo pipefail

run="${1:?usage: scripts/publish.sh <run-id of a green Build run>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "→ artifacts of run $run"
gh run download "$run" --repo piLeoni/drawland --dir "$work"

echo "→ crates.io"
(cd "$root" && cargo publish --workspace)

echo "→ PyPI"
mkdir -p "$work/dist"
cp "$work"/wheel-*/* "$work/dist/"
uvx twine upload "$work"/dist/*

echo "→ npm"
node="$root/bindings/node"
cp "$work"/addon-*/*.node "$node/"
cp "$root/LICENSE" "$node/"
(cd "$node" && npm publish --access public)

echo "Done."
