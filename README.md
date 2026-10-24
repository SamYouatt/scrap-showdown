# Scrapheap Rumble

A Bevy battle-bot game. Build a robot from junk, fight five opponents, and raid
the scrapheap for upgrades. Runs natively or in a WebGL2 browser.

## Develop in an Amp orb

New orbs run `.agents/setup` automatically once these files are on the project's
default branch. To prepare an existing orb, run:

```sh
./.agents/setup
amp orb services ensure
```

Open the **Scrapheap Rumble** link printed by ensure (or use the Portal tab).
The supervised service survives orb pause/resume and CLI updates. The first
Bevy compilation takes several minutes; if ensure times out before it finishes,
check `amp orb service logs game`, then rerun ensure when the build is done.

Changes to `src/`, `assets/`, `web/`, and build configuration automatically
restart the build and server. **Refresh the browser after a successful build**;
this is not hot module replacement. The preview is unavailable while rebuilding.
Compilation failures appear in the service logs and the next edit retries.

```sh
amp orb service logs game
amp orb service status game
amp orb service restart game
amp orb service stop game
```

Setup installs Rust (pinned in `rust-toolchain.toml`), the WASM target, a
`wasm-bindgen` CLI matching `Cargo.lock`, Binaryen, watchexec, and Linux native
build dependencies. It fetches dependencies without compiling the whole game,
keeping snapshot setup short. No secrets or backing services are needed.
No resume hook is needed: Amp supervises the preview.

## Build and check

Run from the repository root:

```sh
./dev-web.sh                        # watch/build/serve, default port 8000
./build-web.sh                      # development browser build into dist/
./build-web.sh --release            # size-optimized browser build, including wasm-opt
cargo check --locked               # native compile check
cargo check --locked --target wasm32-unknown-unknown
cargo test --locked                # native tests
cargo run --locked                 # native game (requires a graphical desktop)
```

In an orb, use the supervised service instead of leaving `./dev-web.sh` running
in a shell. The browser preview is the default way to play; native windows need
a graphical desktop. On another Linux machine, `.agents/setup` assumes Debian
and passwordless sudo; otherwise install the equivalent packages manually.
If Rust is not yet on your current shell's PATH, open a new login shell or source
`$HOME/.cargo/env` before using Cargo directly.

After changing `Cargo.lock`, rerun setup if the `wasm-bindgen` version changed.
Builds use `--locked`, so update the lockfile explicitly when changing dependencies.
WebAssembly packaging defaults to one worker to fit the orb's memory limit;
larger machines can override this with `RAYON_NUM_THREADS`.

### Jump straight to a scene

Append these query strings to the preview URL:

- `?scene=workshop` — robot builder
- `?scene=fight&opp=2&test=1` — test fight against opponent index 2
- `?scene=fight&opp=2&auto=1&test=1` — automated test fight
- `?scene=scrap` — scrapheap

The same options work natively as environment variables, for example
`SCRAP_SCENE=fight SCRAP_OPP=2 SCRAP_TEST=1 cargo run --locked`.
