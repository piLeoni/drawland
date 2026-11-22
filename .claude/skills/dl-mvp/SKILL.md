---
name: dl-mvp
description: Try a new draw.land idea end to end on the command line only — engine change, a profile in configs/, rendered and shown — with no tests, bindings, web page or release. Use when the user floats an idea, asks "try", "prototype", "see how it looks", or a feature is not yet accepted.
---

# Ship level 1: MVP on the CLI

The smallest working version, judged by eye. Nothing outside this repository changes.

## Do

1. Change the engine where the idea lives (`drawland-core`, `drawland-grid`, `drawland-graph`, or a sibling library: build and test that one first, see `.cursor/rules/our-libraries.mdc`).
2. Write or tweak a profile in `configs/` that shows it.
3. Render and look:
   - paper: `cargo run -q --release -p drawland-cli -- map configs/X.js -o /tmp/x.svg && rsvg-convert -w 1400 -b white /tmp/x.svg -o /tmp/x.png`
   - a screen: `drawland map configs/X.js -o x.png` (bitmap) or `-o -` (text), or the moving map: see the `dl-serve` skill.
   - Look at the PNG yourself (Read), then show the user. For "does it move right" questions, measure with a frame-diff script against `drawland serve` before claiming a fix.
4. Iterate on what the user says.

## Don't

- No new test suites, no graphical comparison tests.
- No `bindings/`, no `../drawland-web`, no docs beyond a comment in the profile.
- No commit unless asked; no push.

## Done when

The user says it looks right. Then offer the next level: `dl-engine`.
