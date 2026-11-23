#!/usr/bin/env bash
# Publish a release from green `Build` runs: the crates to crates.io, the
# wheels to PyPI, the Node package to npm. Needs `cargo login`, a PyPI token
# in ~/.pypirc and `npm login`.
#
#   scripts/publish.sh <run-id> [<run-id> …]     # gh run list --workflow build.yml
#   scripts/publish.sh --npm <run-id> …          # npm only
#
# On npm each platform's addon is a package of its own, in the `@drawland`
# organisation (`@drawland/native-darwin-arm64`, `@drawland/native-linux-x64-gnu`,
# …, named by `napi.packageName`, made here by `napi create-npm-dirs`), and
# `drawland` names them all as optional dependencies: npm installs only the
# one for the machine, about 25 MB, not all five. Their versions are the one
# in bindings/node/package.json, which its `optionalDependencies` must match.
#
# Several runs of the same code may be given (GitHub's macOS runners do not
# always start): each package is taken from the first run that has it, and
# the release stops if any platform is missing.
set -euo pipefail

only=""
if [ "${1:-}" = "--npm" ]; then only=npm; shift; fi
[ $# -ge 1 ] || { echo "usage: scripts/publish.sh [--npm] <run-id> [<run-id> …]" >&2; exit 1; }
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
  if [ -z "$only" ] && [ ! -d "$work/all/wheel-aarch64-apple-darwin" ]; then
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
[ -z "$only" ] || want_wheels=""
for a in $want_wheels $want_addons; do
  [ -d "$work/all/$a" ] || { echo "missing $a: rerun the Build workflow and pass that run too" >&2; exit 1; }
done

node="$root/bindings/node"
# The optional dependencies name this very version, or npm would install an
# addon of another release.
version="$(node -p "require('$node/package.json').version")"
node -e "
  const p = require('$node/package.json')
  const off = Object.entries(p.optionalDependencies || {}).filter(([, v]) => v !== p.version)
  if (off.length || !Object.keys(p.optionalDependencies || {}).length) { console.error('bindings/node/package.json: optionalDependencies must all be ' + p.version); process.exit(1) }
"

if [ -z "$only" ]; then
  echo "→ crates.io"
  (cd "$root" && cargo publish --workspace)

  echo "→ PyPI"
  mkdir -p "$work/dist"
  for a in $want_wheels; do cp "$work/all/$a"/* "$work/dist/"; done
  uvx twine upload "$work"/dist/*
fi

echo "→ npm $version: one package per platform"
rm -rf "$node/npm" "$node"/*.node
(cd "$node" && npm install --silent && npx napi create-npm-dirs)
for a in $want_addons; do
  for f in "$work/all/$a"/*.node; do
    platform="$(basename "$f" .node)"
    platform="${platform#drawland.}"
    [ -d "$node/npm/$platform" ] || { echo "no package for $f" >&2; exit 1; }
    cp "$f" "$node/npm/$platform/"
    cp "$root/LICENSE" "$node/npm/$platform/"
  done
done
# A package already on npm at this version is skipped, so a release that
# stopped halfway (npm refused one package) is finished by running it again.
# `drawland` goes last: before it, nobody installs the new addons.
published() { [ "$(npm view "$1@$version" version 2>/dev/null)" = "$version" ]; }
for d in "$node"/npm/*/; do
  ls "$d"*.node >/dev/null || { echo "no addon in $d" >&2; exit 1; }
  name="$(node -p "require('${d}package.json').name")"
  if published "$name"; then echo "  $name@$version is on npm already"; continue; fi
  (cd "$d" && npm publish --access public)
done

echo "→ npm $version: drawland"
cp "$root/LICENSE" "$node/"
if published drawland; then echo "  drawland@$version is on npm already"; else (cd "$node" && npm publish --access public); fi

echo "Done."
