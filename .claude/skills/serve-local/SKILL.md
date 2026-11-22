---
name: serve-local
description: Run draw.land locally for the user to look at — the draw.land web page (Vite dev server from ../drawland-web) and the moving screen maps (drawland serve for 1-bit and terminal styles) — rebuilt from the current code. Use when the user says "serve", "run it", "let me see", "start the servers", or after a change they need to try.
---

# Serve locally

## Start (each as a background task; stop the old one first if it is running)

1. Rebuild the binary when engine code changed: `cargo build --release -p drawland-cli` (in `../drawland`).
2. Screen maps (from `../drawland`):
   - 1-bit: `./target/release/drawland serve configs/1-bit.js --port 8701`
   - terminal: `./target/release/drawland serve configs/terminal.js --port 8702`
   Each offers the other screen styles beside it (style menu).
3. The web page (from `../drawland-web/web`), after `./web/build-wasm.sh` if the engine changed:
   `npx vite --port 5173 --strictPort --host 127.0.0.1`
4. Wait for each to answer 200 (`curl -s -o /dev/null -w "%{http_code}" URL`), then give the user the links:
   - http://127.0.0.1:5173/ (map), http://127.0.0.1:5173/docs (docs; a section by `#id`)
   - http://localhost:8701/ (1-bit), http://localhost:8702/ (terminal); settings in the address: `?column=1&theme=ink&style=…&size=…`, place and scale in the hash: `#lat,lng,scale`

## Notes

- If a port is taken by a process you did not start, say so and do not kill it.
- `drawland view configs/terminal.js` is the same map inside a terminal (the user runs it; it needs a TTY).
- For a phone check, emulate with Chrome's DevTools protocol (390×844 @3x, touch); WebGL needs `--use-angle=swiftshader --enable-unsafe-swiftshader`.
