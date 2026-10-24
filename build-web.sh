#!/usr/bin/env bash
# Build the web version into dist/.  ./build-web.sh [--release]
set -euo pipefail
cd "$(dirname "$0")"
source "$HOME/.cargo/env"
PROFILE=dev
DIR=debug
if [[ "${1:-}" == "--release" ]]; then PROFILE=wasm-release; DIR=wasm-release; fi
cargo build --locked --target wasm32-unknown-unknown --profile "$PROFILE"
rm -rf dist.tmp && mkdir -p dist.tmp
# Bevy's large WASM module makes parallel bindgen passes memory-hungry in orbs.
RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-1}" wasm-bindgen --target web --no-typescript --out-dir dist.tmp --out-name scrapheap "target/wasm32-unknown-unknown/$DIR/scrapheap.wasm"
if [[ "$PROFILE" == wasm-release ]]; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals -o dist.tmp/scrapheap_bg.wasm dist.tmp/scrapheap_bg.wasm
fi
cp web/index.html dist.tmp/
cp -r assets dist.tmp/assets
rm -rf dist && mv dist.tmp dist
ls -la dist
