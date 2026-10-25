#!/usr/bin/env bash
# Regenerate assets/models/*.glb from art/parts.py with headless Blender.
# Usage: ./art/build.sh [part ...]   (ART_TEX=512 ART_SAMPLES=16 for quick drafts)
set -euo pipefail
cd "$(dirname "$0")"
BLENDER="${BLENDER:-$(command -v blender || echo "$HOME/.local/blender/blender")}"
exec "$BLENDER" -b --factory-startup --python-exit-code 1 --python parts.py -- "$@"
