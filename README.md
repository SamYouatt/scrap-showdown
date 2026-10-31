# Scrap Showdown

A collection of small game ideas, one per folder.

| Game | Description |
| --- | --- |
| [Scrapheap Rumble](scrapheap-rumble/) | Bevy battle-bot game: build a robot from junk and fight. |
| [Scrap Crawlers](scrap-crawlers/) | TypeScript + WebGPU prototype: IK-driven procedural scrap creatures rendered as isometric pixel art. |

Orb setup (`.agents/setup`) and the preview services (`.amp/services.yaml`)
live at the repository root and point into each game's folder.

## Getting started

**In an Amp orb**, new orbs run `.agents/setup` automatically. It installs
Rust, Blender and the browser build tools for Scrapheap Rumble, plus the Node
dependencies for Scrap Crawlers. Then start the previews:

```sh
amp orb services ensure            # starts every game and prints portal links
```

Scrap Crawlers is ready in seconds. Scrapheap Rumble's first Rust build takes
several minutes. To work on only one game, stop the other one, for example
`amp orb service stop game`. To bring one back, use
`amp orb service restart crawlers`.

**Locally**, each game is self-contained:

- **Scrap Crawlers** needs Node 20.19+ (or 22.12+) and pnpm:
  `cd scrap-crawlers && pnpm install && pnpm dev`. Open it in a browser with
  WebGPU.
- **Scrapheap Rumble** needs Rust. See [its README](scrapheap-rumble/README.md).
