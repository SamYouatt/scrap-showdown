import { Camera } from './render/camera';
import { DrawList, Mat } from './render/drawlist';
import { Prim, primitiveMeshes } from './render/mesh';
import { RenderOptions, Renderer } from './render/renderer';
import { Creature, Design, PALETTES, defaultDesign, randomDesign } from './creature';
import { World } from './world';
import { V3, add, clamp, damp, mix, norm, qAxisAngle, qIdentity, rng, scale, wrapAngle } from './math';

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('gpu') as HTMLCanvasElement;
const overlay = document.getElementById('overlay') as HTMLCanvasElement;
const hud = document.getElementById('hud') as HTMLDivElement;

interface Settings extends RenderOptions {
  pixelSize: number;
  debug: boolean;
  wander: boolean;
}
const settings: Settings = {
  pixelSize: Number(params.get('pixel') ?? 3),
  outlines: params.get('outlines') !== '0',
  palette: params.get('palette') === '1',
  shadows: params.get('shadows') !== '0',
  debug: params.get('debug') === '1',
  wander: params.get('wander') !== '0',
};

let renderer: Renderer;
try {
  renderer = await Renderer.create(canvas);
} catch (e) {
  (document.getElementById('nogpu') as HTMLDivElement).style.display = 'grid';
  (document.getElementById('nogpu-msg') as HTMLParagraphElement).textContent = String((e as Error).message ?? e);
  throw e;
}

const world = new World(11);
renderer.setMeshes([...primitiveMeshes(), world.mesh]);

let design: Design = params.has('seed') ? randomDesign(Number(params.get('seed'))) : defaultDesign();
let creature = new Creature(design, world, [0, 0, 0], 0.6);
const cam = new Camera();
cam.target = [...creature.frame.p] as V3;
const list = new DrawList();

function rebuild(next: Design) {
  design = next;
  creature = new Creature(design, world, creature.pos, creature.yaw);
  buildControls();
}

// ---------------------------------------------------------------- controls panel

type Ctrl =
  | { kind: 'range'; key: keyof Design | keyof Settings; label: string; min: number; max: number; step: number; target: 'design' | 'settings' }
  | { kind: 'select'; key: keyof Design; label: string; options: [string | number, string][] }
  | { kind: 'check'; key: keyof Settings; label: string }
  | { kind: 'heading'; label: string };

const R = (key: keyof Design, label: string, min: number, max: number, step: number): Ctrl => ({ kind: 'range', key, label, min, max, step, target: 'design' });
const S = (key: keyof Design, label: string, options: [string | number, string][]): Ctrl => ({ kind: 'select', key, label, options });
const C = (key: keyof Settings, label: string): Ctrl => ({ kind: 'check', key, label });
const H = (label: string): Ctrl => ({ kind: 'heading', label });
const opts = (...xs: string[]): [string, string][] => xs.map((x) => [x, x]);

const CONTROLS: Ctrl[] = [
  H('Chassis'),
  S('hull', 'Hull', opts('crate', 'drum', 'pod')),
  R('bodyLength', 'Length', 0.6, 2.4, 0.05),
  R('bodyWidth', 'Width', 0.4, 1.2, 0.05),
  S('palette', 'Paint', PALETTES.map((p, i) => [i, p.name])),
  H('Legs'),
  R('legPairs', 'Pairs', 1, 4, 1),
  R('legSegments', 'Segments', 2, 4, 1),
  R('legLength', 'Length', 0.6, 1.8, 0.05),
  R('legSpread', 'Spread', 0.5, 1.5, 0.05),
  S('legStyle', 'Build', opts('pipe', 'girder', 'piston')),
  H('Gait'),
  S('gait', 'Pattern', [['trot', 'trot (neighbours planted)'], ['creep', 'creep (one at a time)'], ['scurry', 'scurry (half at once)']]),
  R('walkSpeed', 'Walk speed', 0.5, 3, 0.1),
  R('stepSpeed', 'Step speed', 0.5, 2, 0.05),
  R('stepHeight', 'Step height', 0.05, 0.7, 0.05),
  R('rideHeight', 'Ride height', 0.15, 0.8, 0.05),
  H('Extras'),
  S('head', 'Head', opts('can', 'tv', 'lamp')),
  R('tail', 'Tail links', 0, 12, 1),
  R('antennae', 'Antennae', 0, 2, 1),
  H('Render'),
  { kind: 'range', key: 'pixelSize', label: 'Pixel size', min: 1, max: 6, step: 1, target: 'settings' },
  C('outlines', 'Outlines'),
  C('shadows', 'Shadows'),
  C('palette', 'Dither'),
  C('debug', 'IK debug'),
  C('wander', 'Wander'),
];

function buildControls() {
  const root = document.getElementById('controls') as HTMLDivElement;
  root.innerHTML = '';
  for (const c of CONTROLS) {
    if (c.kind === 'heading') {
      const h = document.createElement('h2');
      h.textContent = c.label;
      root.append(h);
      continue;
    }
    const label = document.createElement('label');
    label.append(c.label);
    if (c.kind === 'range') {
      const obj = (c.target === 'design' ? design : settings) as unknown as Record<string, number>;
      const input = Object.assign(document.createElement('input'), { type: 'range', min: c.min, max: c.max, step: c.step, value: obj[c.key] });
      const v = Object.assign(document.createElement('span'), { className: 'v', textContent: String(obj[c.key]) });
      input.addEventListener('input', () => {
        v.textContent = input.value;
        if (c.target === 'design') rebuild({ ...design, [c.key]: Number(input.value) });
        else (settings as unknown as Record<string, number>)[c.key] = Number(input.value);
      });
      label.append(input, v);
    } else if (c.kind === 'select') {
      const sel = document.createElement('select');
      for (const [value, text] of c.options) sel.append(new Option(text, String(value)));
      sel.value = String(design[c.key]);
      sel.addEventListener('change', () => {
        const numeric = typeof c.options[0][0] === 'number';
        rebuild({ ...design, [c.key]: numeric ? Number(sel.value) : sel.value });
      });
      label.append(sel);
    } else {
      label.className = 'check';
      const input = Object.assign(document.createElement('input'), { type: 'checkbox', checked: settings[c.key] });
      input.addEventListener('change', () => ((settings as unknown as Record<string, boolean>)[c.key] = input.checked));
      label.append(input);
    }
    root.append(label);
  }
  const buttons = document.createElement('div');
  buttons.className = 'buttons';
  const button = (text: string, fn: () => void) => {
    const b = document.createElement('button');
    b.textContent = text;
    b.addEventListener('click', fn);
    buttons.append(b);
  };
  button('Re-roll scrap', () => rebuild({ ...design, seed: (Math.random() * 1e6) | 0 }));
  button('Random crawler', () => rebuild(randomDesign((Math.random() * 1e6) | 0)));
  root.append(buttons);
}
buildControls();

// ---------------------------------------------------------------- input

const keys = new Set<string>();
addEventListener('keydown', (e) => {
  if ((e.target as HTMLElement).tagName === 'INPUT' && (e.target as HTMLInputElement).type === 'range') return;
  const k = e.key.toLowerCase();
  keys.add(k);
  if (k === 'q') cam.targetYaw -= Math.PI / 2;
  if (k === 'e') cam.targetYaw += Math.PI / 2;
  if (k === 'f') settings.debug = !settings.debug;
});
addEventListener('keyup', (e) => keys.delete(e.key.toLowerCase()));
addEventListener('blur', () => keys.clear());

const dpr = () => window.devicePixelRatio || 1;
const pick = (e: PointerEvent) => {
  const { origin, dir } = cam.ray(e.clientX * dpr(), e.clientY * dpr());
  return world.raycast(origin, dir);
};
let lastManual = -1e9;
let marker: V3 | null = null;
let drag: { x: number; yaw: number; moved: boolean } | null = null;
canvas.addEventListener('contextmenu', (e) => e.preventDefault());
canvas.addEventListener('pointerdown', (e) => {
  if (e.button === 2) {
    drag = { x: e.clientX, yaw: cam.targetYaw, moved: false };
    canvas.setPointerCapture(e.pointerId);
    return;
  }
  const hit = pick(e);
  if (hit) {
    creature.goal = hit;
    marker = hit;
    lastManual = time;
  }
});
canvas.addEventListener('pointermove', (e) => {
  if (drag) {
    cam.targetYaw = drag.yaw + (e.clientX - drag.x) * 0.01;
    cam.yaw = cam.targetYaw;
    return;
  }
  creature.lookPoint = pick(e);
});
canvas.addEventListener('pointerup', () => {
  if (drag) cam.targetYaw = Math.round(cam.targetYaw / (Math.PI / 4)) * (Math.PI / 4);
  drag = null;
});
canvas.addEventListener('pointerleave', () => (creature.lookPoint = null));

// ---------------------------------------------------------------- loop

const wanderRng = rng(5);
let wanderAt = 0;
function wander() {
  if (!settings.wander || keys.size || time - lastManual < 4) return;
  if (creature.goal && time < wanderAt) return;
  if (!creature.goal || time >= wanderAt) {
    const a = wanderRng.range(0, Math.PI * 2), d = wanderRng.range(2, 6);
    const x = clamp(creature.pos[0] + Math.cos(a) * d, -20, 20), z = clamp(creature.pos[2] + Math.sin(a) * d, -20, 20);
    creature.goal = [x, world.heightAt(x, z), z];
    marker = null;
    wanderAt = time + wanderRng.range(3, 6);
  }
}

function keyDrive(): V3 | null {
  let f = 0, s = 0;
  if (keys.has('w') || keys.has('arrowup')) f += 1;
  if (keys.has('s') || keys.has('arrowdown')) f -= 1;
  if (keys.has('d') || keys.has('arrowright')) s += 1;
  if (keys.has('a') || keys.has('arrowleft')) s -= 1;
  if (!f && !s) return null;
  const fwd: V3 = norm([cam.fwd[0], 0, cam.fwd[2]]);
  const right: V3 = [cam.right[0], 0, cam.right[2]];
  return add(scale(fwd, f), scale(right, s));
}

let time = 0;
let last = performance.now();
let fps = 60;
let lastCss = { w: 0, h: 0, dpr: 0 };

function resize() {
  const w = canvas.clientWidth, h = canvas.clientHeight, d = dpr();
  if (w === lastCss.w && h === lastCss.h && d === lastCss.dpr) return;
  lastCss = { w, h, dpr: d };
  for (const c of [canvas, overlay]) {
    c.width = Math.max(1, Math.round(w * d));
    c.height = Math.max(1, Math.round(h * d));
  }
}

function frame(now: number) {
  const dt = Math.min(0.05, (now - last) / 1000);
  last = now;
  time += dt;
  fps = mix(fps, 1 / Math.max(dt, 1e-3), 0.05);

  creature.drive = keyDrive();
  if (creature.drive) {
    creature.goal = null;
    marker = null;
    lastManual = time;
  }
  wander();
  creature.update(dt, time);
  if (!creature.goal) marker = null;

  cam.target = [
    mix(cam.target[0], creature.frame.p[0], damp(3, dt)),
    mix(cam.target[1], creature.frame.p[1] - 0.3, damp(3, dt)),
    mix(cam.target[2], creature.frame.p[2], damp(3, dt)),
  ];
  cam.yaw += wrapAngle(cam.targetYaw - cam.yaw) * damp(8, dt);
  resize();
  cam.update(canvas.width, canvas.height, dpr(), settings.pixelSize);

  list.clear();
  world.draw(list);
  creature.draw(list);
  if (marker) {
    const bob = 0.25 + Math.sin(time * 6) * 0.06;
    list.add(Prim.Cone, add(marker, [0, bob, 0]), qAxisAngle([1, 0, 0], Math.PI), [0.18, 0.25, 0.18], [1, 0.75, 0.3], Mat.Emissive);
    list.add(Prim.Cyl, add(marker, [0, 0.01, 0]), qIdentity(), [0.35, 0.02, 0.35], [1, 0.75, 0.3], Mat.Emissive);
  }

  // Keep the light over the viewer's left shoulder whichever way the camera faces.
  const ly = cam.yaw - Math.PI / 4;
  const lightDir = norm([-0.35 * Math.cos(ly) + 0.55 * Math.sin(ly), 1, -0.35 * -Math.sin(ly) + 0.55 * Math.cos(ly)] as V3);
  renderer.render(cam, list, settings, lightDir, time);
  drawOverlay();

  const stepping = creature.legs.filter((l) => l.stepping).length;
  hud.innerHTML =
    `<b>click</b> walk here  <b>WASD</b> drive  <b>Q/E</b> or right-drag rotate  <b>F</b> IK debug\n` +
    `${fps.toFixed(0)} fps · ${cam.lowW}×${cam.lowH} px canvas · ${creature.legs.length} legs, ${stepping} in the air · ${renderer.adapterName}`;
  requestAnimationFrame(frame);
}

function drawOverlay() {
  const g = overlay.getContext('2d')!;
  g.clearRect(0, 0, overlay.width, overlay.height);
  if (!settings.debug) return;
  const d = dpr();
  g.lineWidth = 1.5 * d;
  const P = (p: V3) => cam.project(p);
  for (const l of creature.legs) {
    // Step trigger radius around the ideal foothold.
    g.strokeStyle = l.stepping ? 'rgba(255,200,80,.9)' : 'rgba(120,220,255,.6)';
    g.beginPath();
    for (let i = 0; i <= 24; i++) {
      const a = (i / 24) * Math.PI * 2;
      const [x, y] = P([l.desired[0] + Math.cos(a) * creature.stride, l.desired[1], l.desired[2] + Math.sin(a) * creature.stride]);
      if (i) g.lineTo(x, y);
      else g.moveTo(x, y);
    }
    g.stroke();
    const [dx, dy] = P(l.desired);
    g.fillStyle = 'rgba(120,220,255,.9)';
    g.fillRect(dx - 2 * d, dy - 2 * d, 4 * d, 4 * d);
    // The IK chain itself.
    g.strokeStyle = '#ffe14d';
    g.beginPath();
    l.joints.forEach((j, i) => {
      const [x, y] = P(j);
      if (i) g.lineTo(x, y);
      else g.moveTo(x, y);
    });
    g.stroke();
    g.fillStyle = '#ff5a3d';
    for (const j of l.joints) {
      const [x, y] = P(j);
      g.fillRect(x - 2.5 * d, y - 2.5 * d, 5 * d, 5 * d);
    }
  }
  const [bx, by] = P(creature.frame.p);
  g.fillStyle = '#fff';
  g.font = `${11 * d}px ui-monospace, monospace`;
  g.fillText(`stride ${creature.stride.toFixed(2)}  gait ${creature.design.gait}`, bx + 30 * d, by - 30 * d);
}

requestAnimationFrame(frame);
