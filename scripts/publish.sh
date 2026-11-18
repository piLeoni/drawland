#!/usr/bin/env bash
# Publish a release from green `Build` runs: the crates to crates.io, the
# wheels to PyPI, the Node package (every platform's addon in one package)
# to npm. Needs `cargo login`, a PyPI token in ~/.pypirc and `npm login`.
#
#   scripts/publish.sh <run-id> [<run-id> …]     # gh run list --workflow build.yml
#
# Several runs of the same code may be given (GitHub's macOS runners do not
# always start): each package is taken from the first run that has it, and
# the release stops if any platform is missing.
set -euo pipefail

[ $# -ge 1 ] || { echo "usage: scripts/publish.sh <run-id> [<run-id> …]" >&2; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for run in "$@"; do
  echo "→ artifacts of run $run"
  gh run download "$run" --repo piLeoni/drawland --dir "$work/$run" || true
done

# The first run's copy of each artifact.
mkdir -p "$work/all"
for run in "$@"; do
  for a in "$work/$run"/*/; do
    [ -d "$a" ] || continue
    name="$(basename "$a")"
    [ -e "$work/all/$name" ] || cp -R "$a" "$work/all/$name"
  done
done

# GitHub's macOS arm64 runners often never start: on an Apple silicon Mac,
# what they would have built is built here.
if [ "$(uname -s)-$(uname -m)" = "Darwin-arm64" ]; then
  export MACOSX_DEPLOYMENT_TARGET=11.0
  if [ ! -d "$work/all/wheel-aarch64-apple-darwin" ]; then
    echo "→ the macOS arm64 wheel, here"
    (cd "$root/bindings/python" && cp "$root/LICENSE" . && uvx maturin build --release --out "$work/all/wheel-aarch64-apple-darwin")
  fi
  if [ ! -d "$work/all/addon-aarch64-apple-darwin" ]; then
    echo "→ the macOS arm64 Node addon, here"
    (cd "$root/bindings/node" && npm install --silent && npx napi build --platform --release --target aarch64-apple-darwin --js native.js --dts native.d.ts)
    mkdir -p "$work/all/addon-aarch64-apple-darwin"
    cp "$root/bindings/node/drawland.darwin-arm64.node" "$work/all/addon-aarch64-apple-darwin/"
  fi
fi

want_wheels="wheel-x86_64 wheel-aarch64 wheel-aarch64-apple-darwin wheel-x86_64-apple-darwin wheel-x64 wheel-sdist"
want_addons="addon-aarch64-apple-darwin addon-x86_64-apple-darwin addon-x86_64-unknown-linux-gnu addon-aarch64-unknown-linux-gnu addon-x86_64-pc-windows-msvc"
for a in $want_wheels $want_addons; do
  [ -d "$work/all/$a" ] || { echo "missing $a: rerun the Build workflow and pass that run too" >&2; exit 1; }
done

echo "→ crates.io"
(cd "$root" && cargo publish --workspace)

echo "→ PyPI"
mkdir -p "$work/dist"
for a in $want_wheels; do cp "$work/all/$a"/* "$work/dist/"; done
uvx twine upload "$work"/dist/*

echo "→ npm"
node="$root/bindings/node"
rm -f "$node"/*.node
for a in $want_addons; do cp "$work/all/$a"/*.node "$node/"; done
cp "$root/LICENSE" "$node/"
(cd "$node" && npm publish --access public)

echo "Done."
