# Gallery3D

A small first-person 3D demo built with [macroquad](https://github.com/not-fl3/macroquad),
targeting the web via WebAssembly. Walk through a 3-room art gallery and view
a sampling of renders from a ray tracer project, displayed as framed wall
paintings under simulated gallery spotlights.

**[Play it in your browser »](https://k2msmith-gfx.github.io/gallery3d/)**

## Controls

- **Up / Down** — walk forward / backward
- **Left / Right** — turn

## How it's rendered

macroquad's default 3D pipeline has no built-in lighting, so lighting here is
baked per-vertex on the CPU: each wall/floor/ceiling quad is subdivided into a
grid, and every vertex is shaded from an ambient term plus a handful of
attenuated point lights (one ceiling light per room, plus a small spotlight
over each painting). Walls, floor, and ceiling use small procedurally
generated textures (no external assets besides the gallery photos).

## Running locally

Native (fastest iteration loop):

```sh
cargo run --release
```

Web (produces a static site in `docs/`):

```sh
./scripts/build-web.sh
python3 -m http.server --directory docs 8080
# open http://localhost:8080
```

## Project layout

- `src/main.rs` — the whole demo: level geometry, lighting, procedural
  textures, player movement/collision, and the render loop.
- `assets/images/` — a curated set of renders pulled from a ray tracer
  project's examples, resized/recompressed for the web.
- `docs/` — the static site: `index.html`, the vendored (and hand-patched,
  see comments in the file) macroquad JS glue, the built `.wasm` binary, and
  a copy of `assets/`. This is what's published to GitHub Pages (from
  `main`, `/docs`).
- `scripts/build-web.sh` — rebuilds the wasm binary and refreshes `docs/`.
