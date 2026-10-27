#!/usr/bin/env bash
# Regenerate assets/models/*.glb with headless Blender (4.5 LTS or newer).
# Usage: ./art/build.sh [asset|module ...]   e.g. ./art/build.sh wheel arena
#   ART_TEX=1024      default texture size (assets may override)
#   ART_SAMPLES=16    bake samples
#   ART_PREVIEW=dir   also render a quick look at each asset into dir/<name>.png
#   ART_GPU=1         bake on the GPU (slower for small bakes: kernel compile)
set -euo pipefail
cd "$(dirname "$0")"
BLENDER="${BLENDER:-$(command -v blender || echo "$HOME/.local/blender/blender")}"
exec "$BLENDER" -b --factory-startup --python-exit-code 1 --python build.py -- "$@"
