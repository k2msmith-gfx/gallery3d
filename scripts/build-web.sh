#!/usr/bin/env bash
# Builds the wasm32 release binary and assembles a self-contained static
# site in web/ (wasm binary, JS glue, html shell, and the gallery assets).
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

cargo build --release --target wasm32-unknown-unknown

cp target/wasm32-unknown-unknown/release/gallery3d.wasm docs/gallery3d.wasm
rm -rf docs/assets
cp -R assets docs/assets

echo "Static site assembled in docs/ (served by GitHub Pages). Preview it with:"
echo "  python3 -m http.server --directory docs 8080"
