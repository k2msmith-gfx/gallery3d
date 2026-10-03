#!/usr/bin/env bash
# Builds the wasm32 release binary and assembles a self-contained static
# site in web/ (wasm binary, JS glue, html shell, and the gallery assets).
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

cargo build --release --target wasm32-unknown-unknown

cp target/wasm32-unknown-unknown/release/gallery3d.wasm web/gallery3d.wasm
rm -rf web/assets
cp -R assets web/assets

echo "Static site assembled in web/. Serve it with, e.g.:"
echo "  python3 -m http.server --directory web 8080"
