#!/usr/bin/env bash
# Rebuild on edits, then serve the game. Refresh the browser after each build.
set -euo pipefail
cd "$(dirname "$0")"
source "$HOME/.cargo/env"
export PORT="${PORT:-8000}"
exec watchexec --restart --debounce 500ms --shell=none \
  --watch src --watch assets --watch web --watch Cargo.toml --watch Cargo.lock \
  --watch .cargo --watch rust-toolchain.toml --watch build-web.sh \
  -- sh -c './build-web.sh && exec python3 -m http.server "$PORT" --bind 0.0.0.0 --directory dist'
