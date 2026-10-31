// The workshop: pick a body, then drag parts from the bin onto its mounts.
// The build on the turntable is a live creature, so legs settle, the head
// watches the cursor and tools animate while you work.
import { Camera } from './render/camera';
import { DrawList, Mat } from './render/drawlist';
import { Prim, primitiveMeshes } from './render/mesh';
import { Renderer } from './render/renderer';
import { Creature, hullDims } from './creature';
import { BODIES, Build, PARTS, PartDef, SLOT_NAMES, SLOT_ORDER, Section, decodeBuild, designFor, encodeBuild, part, problems } from './parts';
import { Mount, SlotId, mountPoints } from './tools';
import { WorkshopRoom } from './room';
import { V3, add, clamp, mix, norm, qAlongY, scale, toWorld } from './math';

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('gpu') as HTMLCanvasElement;
const overlay = document.getElementById('overlay') as HTMLCanvasElement;
const hud = document.getElementById('hud') as HTMLDivElement;
const ghost = document.getElementById('ghost') as HTMLDivElement;
const prompt = document.getElementById('prompt') as HTMLDivElement;

let renderer: Renderer;
try {
  renderer = await Renderer.create(canvas);
} catch (e) {
  (document.getElementById('nogpu') as HTMLDivElement).style.display = 'grid';
  (document.getElementById('nogpu-msg') as HTMLParagraphElement).textContent = String((e as Error).message ?? e);
  throw e;
}

const room = new WorkshopRoom();
renderer.setMeshes([...primitiveMeshes(), room.mesh]);
const settings = { pixelSize: Number(params.get('pixel') ?? 3), outlines: true, palette: false, shadows: true };

const cam = new Camera();
cam.yaw = cam.targetYaw = 1.05;
cam.zoom = 150;
cam.target = [0, 0.6, 0];
const list = new DrawList();

// ---------------------------------------------------------------- build state

let build: Build = decodeBuild(params.get('build'));
/** The part being dragged, the mount it came from (if it was fitted) and the mount it would drop onto. */
let drag: { part: PartDef; from: SlotId | null; snap: SlotId | null } | null = null;
let creature: Creature | null = null;
let shownKey = '';

/** Rebuilds the creature only when the build actually changes. */
function refresh() {
  const b = build;
  const key = encodeBuild(b);
  if (key === shownKey) return;
  shownKey = key;
  const look = creature?.lookPoint ?? null;
  creature = b.body ? new Creature(designFor(b), room, [0, 0, 0], 0) : null;
  if (creature) creature.lookPoint = look;
}

function commit(next: Build) {
  build = next;
  const url = new URL(location.href);
  if (build.body) url.searchParams.set('build', encodeBuild(build));
  else url.searchParams.delete('build');
  history.replaceState(null, '', url);
  refresh();
  renderBin();
  renderSheet();
}

// ---------------------------------------------------------------- parts bin

const SECTIONS: [Section, string][] = [['legs', 'Leg kits'], ['weapons', 'Weapons & tools'], ['control', 'Control modules']];

function chip(text: string, cls = '') {
  return `<span class="${cls}">${text}</span>`;
}

function card(p: PartDef) {
  const fitted = Object.values(build.slots).includes(p.id);
  const wanted = p.under && Object.values(build.slots).includes(p.under) && !fitted;
  const el = document.createElement('div');
  el.className = `card${p.under ? ' nested' : ''}${fitted ? ' fitted' : ''}${wanted ? ' wanted' : ''}${build.body ? '' : ' locked'}`;
  el.innerHTML = `<div class="name">${p.name}</div><div class="blurb">${p.blurb}</div><div class="chips">` +
    p.stats.map((s) => chip(s)).join('') +
    chip(p.fits.map((s) => SLOT_NAMES[s]).join(' / '), 'slot') +
    (p.needs ? chip(`needs ${part(p.needs)!.name}`, 'needs') : '') + '</div>';
  el.addEventListener('pointerdown', (e) => {
    if (!build.body || e.button !== 0) return;
    e.preventDefault();
    startDrag(p, null, e);
  });
  return el;
}

function renderBin() {
  const root = document.getElementById('parts') as HTMLDivElement;
  root.innerHTML = '';
  const h = (t: string) => Object.assign(document.createElement('h2'), { textContent: t });
  root.append(h('Body'));
  for (const b of BODIES) {
    const el = document.createElement('div');
    el.className = `card body${b.ready ? '' : ' locked soon'}${build.body === b.id ? ' chosen fitted' : ''}`;
    el.innerHTML = `<div class="name">${b.name}</div><div class="blurb">${b.blurb}</div>`;
    if (b.ready) el.addEventListener('click', () => build.body !== b.id && commit({ body: b.id, slots: {} }));
    root.append(el);
  }
  for (const [section, title] of SECTIONS) {
    root.append(h(title));
    for (const p of PARTS.filter((x) => x.section === section && !x.under)) {
      root.append(card(p));
      for (const child of PARTS.filter((x) => x.under === p.id)) root.append(card(child));
    }
  }
}

function renderSheet() {
  const root = document.getElementById('sheet') as HTMLDivElement;
  prompt.style.display = build.body ? 'none' : 'block';
  prompt.innerHTML = 'Pick a <b>body</b> from the parts bin to start a build';
  root.style.display = build.body ? 'block' : 'none';
  if (!build.body) return;
  root.innerHTML = `<h1>Build sheet</h1><div class="sub">${BODIES.find((b) => b.id === build.body)!.name}</div>`;
  for (const slot of SLOT_ORDER) {
    const row = document.createElement('div');
    row.className = 'row';
    const id = build.slots[slot];
    row.innerHTML = `<span class="slot">${SLOT_NAMES[slot]}</span>` + (id ? `<span>${part(id)!.name}</span>` : '<span class="empty">empty</span>');
    if (id) {
      const x = Object.assign(document.createElement('button'), { textContent: '×', title: 'Remove' });
      x.addEventListener('click', () => commit(without(slot)));
      row.append(x);
    }
    root.append(row);
  }
  const issues = problems(build);
  for (const msg of issues) root.append(Object.assign(document.createElement('div'), { className: 'warn', textContent: `! ${msg}` }));
  if (!issues.length) root.append(Object.assign(document.createElement('div'), { className: 'ok', textContent: 'Ready to crawl.' }));
  const buttons = document.createElement('div');
  buttons.className = 'buttons';
  const button = (text: string, fn: () => void) => {
    const b = Object.assign(document.createElement('button'), { textContent: text });
    b.addEventListener('click', fn);
    buttons.append(b);
  };
  button('Strip parts', () => commit({ body: build.body, slots: {} }));
  button('Scrap it', () => commit({ body: null, slots: {} }));
  root.append(buttons);
}

function without(slot: SlotId): Build {
  const slots = { ...build.slots };
  delete slots[slot];
  return { body: build.body, slots };
}

// ---------------------------------------------------------------- mounts

const dpr = () => window.devicePixelRatio || 1;
/** Pointer position in canvas device pixels. */
const px = (e: PointerEvent): [number, number] => {
  const r = canvas.getBoundingClientRect();
  return [(e.clientX - r.left) * dpr(), (e.clientY - r.top) * dpr()];
};

function mounts(): Mount[] {
  return creature ? mountPoints(hullDims(creature.design)) : [];
}

/** Screen position (device pixels) of a hull-local point on the live creature. */
function screen(local: V3): [number, number] {
  return cam.project(toWorld(creature!.frame, local));
}

/** Nearest mount (within reach of the pointer) whose slot passes the filter. */
function nearestMount(e: PointerEvent, accept: (m: Mount) => boolean, at: (m: Mount) => V3): Mount | null {
  let best: Mount | null = null, bestD = 70 * dpr();
  for (const m of mounts()) {
    if (!accept(m)) continue;
    const [x, y] = screen(at(m));
    const [ex, ey] = px(e);
    const d = Math.hypot(x - ex, y - ey);
    if (d < bestD) {
      best = m;
      bestD = d;
    }
  }
  return best;
}

let hover: SlotId | null = null;

function drawMounts(time: number) {
  if (!creature) return;
  for (const m of mounts()) {
    const filled = !!build.slots[m.slot];
    let col: V3 = [1, 0.78, 0.16], size = 1, mat = Mat.Emissive;
    if (drag) {
      if (drag.snap === m.slot) {
        // The mount the part will land on when released.
        size = 1.45;
        col = [1, 0.97, 0.7];
      } else if (drag.part.fits.includes(m.slot)) {
        size = 1.15 + Math.sin(time * 8) * 0.12;
        col = [1, 0.86, 0.25];
      } else if (filled) {
        continue;
      } else {
        col = [0.42, 0.38, 0.3];
        size = 0.8;
        mat = Mat.Lit;
      }
    } else if (filled) {
      continue;
    } else if (hover === m.slot) {
      col = [1, 0.97, 0.7];
      size = 1.3;
    } else {
      size = 1 + Math.sin(time * 3) * 0.06;
    }
    // Lift markers clear of whatever is already fitted so swaps stay visible.
    const p = add(m.p, scale(m.n, filled ? 0.14 : 0.012));
    const q = qAlongY(m.n, Math.abs(m.n[0]) > 0.5 ? [0, 0, 1] : [1, 0, 0]);
    list.addLocal(creature.frame, Prim.Cyl, p, q, [0.26 * size, 0.02, 0.26 * size], col, mat);
    list.addLocal(creature.frame, Prim.Cyl, add(p, scale(m.n, 0.006)), q, [0.14 * size, 0.024, 0.14 * size], [0.16, 0.13, 0.1]);
  }
}

function slotLabel(slot: SlotId) {
  const sections = [...new Set(PARTS.filter((p) => p.fits.includes(slot)).map((p) => p.section))];
  const names: Record<Section, string> = { legs: 'leg kits', control: 'control modules', weapons: 'weapons & tools' };
  return [SLOT_NAMES[slot].toUpperCase(), sections.map((s) => names[s]).join(', ')];
}

// ---------------------------------------------------------------- input

function startDrag(p: PartDef, from: SlotId | null, e: PointerEvent) {
  drag = { part: p, from, snap: null };
  ghost.textContent = p.name;
  ghost.style.display = 'block';
  moveDrag(e);
}

function moveDrag(e: PointerEvent) {
  if (!drag) return;
  ghost.style.left = `${e.clientX}px`;
  ghost.style.top = `${e.clientY}px`;
  const over = e.target === canvas;
  const m = over ? nearestMount(e, (x) => drag!.part.fits.includes(x.slot), (x) => x.p) : null;
  drag.snap = m?.slot ?? null;
  ghost.classList.toggle('snapped', !!m);
  ghost.textContent = m ? `${drag.part.name} → ${SLOT_NAMES[m.slot]}` : drag.part.name;
}

function endDrag() {
  if (!drag) return;
  const { part: p, from, snap } = drag;
  drag = null;
  ghost.style.display = 'none';
  // The build only changes on release. Dropping away from a mount pulls a fitted part off.
  const next = from ? without(from) : build;
  if (snap) next.slots[snap] = p.id;
  if (from || snap) commit(next);
}

let orbit: { x: number; y: number; yaw: number; pitch: number } | null = null;

canvas.addEventListener('contextmenu', (e) => e.preventDefault());
canvas.addEventListener('pointerdown', (e) => {
  // Grab a fitted part to move it or pull it off; otherwise orbit the camera.
  if (e.button === 0 && creature) {
    const m = nearestMount(e, (x) => !!build.slots[x.slot], (x) => x.grab);
    if (m) {
      const p = part(build.slots[m.slot]!)!;
      startDrag(p, m.slot, e);
      return;
    }
  }
  orbit = { x: e.clientX, y: e.clientY, yaw: cam.yaw, pitch: cam.pitch };
  canvas.style.cursor = 'grabbing';
});
addEventListener('pointermove', (e) => {
  if (creature) {
    // The head watches the cursor (or the part you're carrying).
    const { origin, dir } = cam.ray(...px(e));
    const t = (0.6 - origin[1]) / dir[1];
    creature.lookPoint = add(origin, scale(dir, t));
  }
  if (drag) return moveDrag(e);
  if (orbit) {
    cam.yaw = cam.targetYaw = orbit.yaw + (e.clientX - orbit.x) * 0.01;
    cam.pitch = clamp(orbit.pitch + (e.clientY - orbit.y) * 0.006, 0.32, 1.15);
    return;
  }
  const over = e.target === canvas;
  const grab = over && creature ? nearestMount(e, (x) => !!build.slots[x.slot], (x) => x.grab) : null;
  const empty = over && creature && !grab ? nearestMount(e, (x) => !build.slots[x.slot], (x) => x.p) : null;
  hover = grab?.slot ?? empty?.slot ?? null;
  canvas.style.cursor = grab ? 'grab' : '';
});
addEventListener('pointerup', () => {
  endDrag();
  orbit = null;
  canvas.style.cursor = '';
});
canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  cam.zoom = clamp(cam.zoom * Math.exp(-e.deltaY * 0.0015), 100, 300);
}, { passive: false });

// ---------------------------------------------------------------- loop

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

  creature?.update(dt, time);
  const ty = creature ? creature.frame.p[1] : room.deck + 0.4;
  cam.target = [0, mix(cam.target[1], ty, 1 - Math.exp(-3 * dt)), 0];
  resize();
  cam.update(canvas.width, canvas.height, dpr(), settings.pixelSize);

  list.clear();
  room.draw(list);
  creature?.draw(list);
  drawMounts(time);

  const ly = cam.yaw - Math.PI / 4;
  const lightDir = norm([-0.35 * Math.cos(ly) + 0.55 * Math.sin(ly), 1, -0.35 * -Math.sin(ly) + 0.55 * Math.cos(ly)] as V3);
  renderer.render(cam, list, settings, lightDir, time);
  drawOverlay();

  hud.innerHTML =
    `<b>drag</b> parts onto mounts  <b>drag a fitted part</b> to move or remove it\n` +
    `<b>left/right-drag</b> orbit  <b>wheel</b> zoom · ${fps.toFixed(0)} fps · ${renderer.adapterName}`;
  requestAnimationFrame(frame);
}

/** Labels the mount under the cursor, or the one a dragged part is snapped to. */
function drawOverlay() {
  const g = overlay.getContext('2d')!;
  g.clearRect(0, 0, overlay.width, overlay.height);
  const slot = drag ? drag.snap : hover;
  if (!creature || !slot) return;
  const m = mounts().find((x) => x.slot === slot && (x.n[0] >= 0 || slot !== 'legs'))!;
  const [x, y] = screen(drag ? m.p : build.slots[slot] ? m.grab : m.p);
  const d = dpr();
  const [title, sub] = slotLabel(slot);
  const fitted = build.slots[slot] && !drag ? part(build.slots[slot]!)!.name : sub;
  g.font = `${12 * d}px ui-monospace, Menlo, monospace`;
  const w = Math.max(g.measureText(title).width, g.measureText(fitted).width) + 16 * d;
  const bx = Math.round(x + 18 * d), by = Math.round(y - 44 * d);
  g.fillStyle = 'rgba(32,27,26,.92)';
  g.fillRect(bx, by, w, 38 * d);
  g.strokeStyle = '#ffd23a';
  g.lineWidth = 2 * d;
  g.strokeRect(bx, by, w, 38 * d);
  g.fillStyle = '#ffd23a';
  g.fillText(title, bx + 8 * d, by + 15 * d);
  g.fillStyle = '#b7a98f';
  g.fillText(fitted, bx + 8 * d, by + 31 * d);
}

renderBin();
renderSheet();
refresh();
requestAnimationFrame(frame);
