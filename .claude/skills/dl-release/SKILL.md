---
name: dl-release
description: Publish draw.land — push the engine, release crates/PyPI/npm packages from the CI build, and/or deploy the draw.land website. Outward-facing and hard to undo; always confirm with the user which targets before acting. Use only when the user asks to push, release, publish, deploy, or put it online.
---

# Ship level 5: release (outward)

Everything here leaves the machine. Before each step, say what will be published and get a yes for that target; a yes for one target is not a yes for another.

## Preconditions

`dl-engine` (and `dl-faces` for packages, `dl-site` for the website) done and green; `main` fast-forwarded; no uncommitted changes of the user's swept in.

## Targets

1. **Push the engine** (`../drawland`, public): `git push origin main` (and the working branch if the user wants it). This starts `.github/workflows/build.yml`.
2. **Packages** (crates.io, PyPI, npm): bump versions together (`Cargo.toml` workspace, `bindings/python/pyproject.toml`, `bindings/node/package.json` and its `optionalDependencies`), push, wait for the Build run (`gh run list --workflow build.yml`, `gh run watch`), then `scripts/publish.sh <run-id>` (several run ids if a platform is missing from one). Known npm blockers: see the project memory (platform addons, the win32 package name).
3. **Website** (`../drawland-web`, private repo, Vercel project `drawland`): push its repo if asked, then `cd web && ./deploy.sh` (builds WASM, checks presets, deploys). Check https://draw.land afterwards.

## Never

- Publish with failing tests or a STALE `graphs.json`.
- Force-push, or publish a target the user did not name.

## Done when

Each named target is published and verified (crate/package page shows the version; the site serves the change). Report the URLs.
