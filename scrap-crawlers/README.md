# Scrap Crawlers

A TypeScript + raw WebGPU prototype of a different direction for the scrap game:
procedurally animated junk creatures drawn as pixel art from an isometric view.
Nothing is hand-drawn or keyframed. The creatures are built from real 3D
primitives, moved by inverse kinematics and gait rules, and the renderer
pixelates them with shaders.

## Run it

In an orb, `amp orb services ensure` starts the `crawlers` service and prints its
portal link. Vite hot-reloads edits. Locally:

```sh
pnpm install
pnpm dev        # http://localhost:5173
pnpm build      # typecheck + production bundle in dist/
```

You need a browser with WebGPU: a recent Chrome, Edge or Safari, or Firefox
Nightly with WebGPU enabled.

Controls: click the ground to walk there, WASD to drive, Q/E or right-drag to
rotate the camera, F to toggle the IK debug overlay. The
panel on the right edits the creature live.

URL options: `?seed=N` (random creature), `?debug=1`, `?wander=0`, `?pixel=1..6`,
`?outlines=0`, `?shadows=0`, `?palette=1` (dither).

### Headless testing (agents, CI)

Headless Chrome has no GPU in an orb, so by default WebGPU finds no adapter, or
the device is lost as soon as it presents. Software WebGPU (SwiftShader) runs at
about 30–50 fps with these flags:

```sh
export AGENT_BROWSER_ARGS="--enable-unsafe-webgpu,--enable-features=Vulkan,--use-vulkan=swiftshader,--use-webgpu-adapter=swiftshader,--ignore-gpu-blocklist,--enable-gpu-compositing,--use-angle=vulkan"
agent-browser open "http://127.0.0.1:<port>/?seed=5&wander=0"
```

Get `<port>` from `amp orb service status crawlers`. Use `127.0.0.1` rather than
`localhost`, because WebGPU needs a secure context.
Flags only apply when the browser launches, so run `agent-browser close --all`
first. The HUD in the bottom-left shows the frame rate and adapter name, which
helps confirm WebGPU is running.

## How it works

**Procedural movement** (`src/creature.ts`, `src/ik.ts`)

- Each foot stays planted until the body moves its ideal foothold more than a
  stride away. The foot then steps along an arc to a predicted landing point,
  which accounts for velocity and turning, and retargets while it is in the air.
- Gait rules decide which legs may lift together. *Trot* keeps neighbours
  planted, *creep* lifts one leg at a time, *scurry* lifts up to half the legs.
  Legs that fall far behind may break the rule, so the creature never gets stuck.
- Legs are FABRIK chains with any number of segments. Each solve starts from a
  curve bent towards a pole: spider-like for many legs, knees forward for bipeds.
- The body rides a spring above the planted feet (each footfall kicks it) and
  tilts to the plane of the feet. The head is a damped spring that looks at the
  cursor. The tail follows the leader, and the antennae are verlet chains.
- Walking surfaces come from `World.heightAt`, so crates, drums and tyres can
  be climbed.

**Pixel rendering** (`src/render/`)

- `camera.ts`: an orthographic camera at a 30° pitch renders into a low-res
  target (one texel = *pixel size* screen pixels). The camera snaps to whole
  texels so static pixels never shimmer. The leftover sub-texel offset is
  applied when upscaling, so the camera still glides smoothly.
- `scene.wgsl`: instanced flat-shaded primitives with three hard toon bands,
  tinted shadows, nearest-filtered shadow maps (themselves texel-snapped) and
  world-locked ground speckle. It writes colour, normals and depth.
- `post.wgsl`: nearest-neighbour upscale plus pixel-art edges. Silhouette texels
  are darkened from depth, convex creases get a one-texel highlight from
  normals, and an optional Bayer dither is locked to the world.

These ideas follow t3ssel8r's 3D pixel-art devlogs on YouTube, and procedural
IK walkers such as argonaut's procedural animation videos.
