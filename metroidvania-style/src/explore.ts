// Explore: a Pokémon-style overworld experiment. Your build roams the scrapyard
// while wild crawlers wander near their dens. One that spots you shouts "!" and
// charges; touching one flashes the screen and wipes to the pit, where the usual
// auto-battle plays out. Win and the wild crawler is scrapped; lose and you are
// towed back to the repair pad. Damage carries over between fights.
import { Camera } from './render/camera';
import { DrawList, Mat } from './render/drawlist';
import { Prim, primitiveMeshes } from './render/mesh';
import { Renderer } from './render/renderer';
import { Battle } from './battle';
import { Creature, hullDims } from './creature';
import { FightHud } from './fighthud';
import { Pit } from './pit';
import { Build, decodeBuild, designFor, encodeBuild } from './parts';
import { World } from './world';
import { V3, add, clamp, damp, distXZ, hex, mix, norm, qAxisAngle, qIdentity, rng, scale, wrapAngle } from './math';

const WILD = [
  { name: 'Plowboy', build: 'box;legs=quad;fronttop=can;front=wedge' },
  { name: 'Snapper', build: 'box;legs=crab;fronttop=tv;front=pincer;spine=pneumatics' },
  { name: 'Backhand', build: 'box;legs=millipede;fronttop=lamp;back=pincer;spine=pneumatics' },
  { name: 'Skitter', build: 'box;legs=strider;fronttop=tv;front=wedge' },
  { name: 'Rustmite', build: 'box;legs=millipede;fronttop=can' },
  { name: 'Nipper', build: 'box;legs=strider;fronttop=lamp;front=pincer;spine=pneumatics' },
];
const DEFAULT_BUILD = 'box;legs=crab;fronttop=tv;front=wedge';
/** How far each control module sees: the lamp-eye spots you from across the yard. */
const SIGHT: Record<string, number> = { can: 4.5, tv: 6, lamp: 8.5 };
/** Wild crawlers amble at this fraction of their speed until they spot you. */
const AMBLE = 0.45;
const PAD_RADIUS = 1.1;

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('gpu') as HTMLCanvasElement;
const overlay = document.getElementById('overlay') as HTMLCanvasElement;
const ui = {
  roam: document.getElementById('roam') as HTMLDivElement,
  fight: document.getElementById('fight') as HTMLDivElement,
  hp: document.querySelector('#status .bar div') as HTMLDivElement,
  line: document.querySelector('#status .line') as HTMLDivElement,
  hint: document.querySelector('#hint > div') as HTMLDivElement,
  fightmenu: document.getElementById('fightmenu') as HTMLDivElement,
  result: document.getElementById('result') as HTMLDivElement,
  resultText: document.querySelector('#result p') as HTMLParagraphElement,
};

let renderer: Renderer;
try {
  renderer = await Renderer.create(canvas);
} catch (e) {
  (document.getElementById('nogpu') as HTMLDivElement).style.display = 'grid';
  (document.getElementById('nogpu-msg') as HTMLParagraphElement).textContent = String((e as Error).message ?? e);
  throw e;
}

const prims = primitiveMeshes();
const world = new World(11);
/** Fights here happen in an arena themed to match this part of the world. */
const pit = new Pit('yard');
/** Where you start, and where you're towed back to after losing. Standing on it repairs you. */
const PAD: V3 = [0, world.ground(0, 0), 0];
renderer.setMeshes([...prims, world.mesh]);
const settings = { pixelSize: Number(params.get('pixel') ?? 3), outlines: true, palette: false, shadows: true };

const decoded = decodeBuild(params.get('build'));
const build: Build = decoded.body ? decoded : decodeBuild(DEFAULT_BUILD);
(document.getElementById('back') as HTMLAnchorElement).href = `./workshop.html?build=${encodeURIComponent(encodeBuild(build))}`;
const spawnPlayer = () => new Creature({ ...designFor(build), palette: 0 }, world, PAD, 0.6);
let player = spawnPlayer();
let hp = 100;
let scrapped = 0;

// ---------------------------------------------------------------- wild crawlers

interface Wild {
  name: string;
  build: Build;
  palette: number;
  creature: Creature;
  home: V3;
  sight: number;
  state: 'wander' | 'alert' | 'chase' | 'return';
  /** Time left in the current state (alert pause, chase patience). */
  t: number;
  wanderAt: number;
  /** "!" when it spots you, "?" when it gives up. */
  mark: { text: string; until: number } | null;
}

let wilds: Wild[] = [];
let wave = 0;

/** Scatters one of each wild crawler in its own clear patch of ground, away from the pad and each other. */
function spawnWilds() {
  const r = rng(101 + wave++);
  const homes: V3[] = [];
  wilds = WILD.map((w, i) => {
    let home: V3 = [0, 0, 0];
    for (let tries = 0; tries < 200; tries++) {
      const a = r.range(0, Math.PI * 2), d = r.range(8, world.half - 6);
      const x = Math.cos(a) * d, z = Math.sin(a) * d;
      const g = world.ground(x, z);
      if (world.heightAt(x, z) > g + 0.01 || homes.some((h) => distXZ(h, [x, 0, z]) < 6)) continue;
      home = [x, g, z];
      break;
    }
    homes.push(home);
    const b = decodeBuild(w.build);
    const palette = 1 + (i % 4);
    const creature = new Creature({ ...designFor(b), palette }, world, home, r.range(0, Math.PI * 2));
    creature.mobility = AMBLE;
    const sight = SIGHT[designFor(b).head ?? 'can'];
    return { name: `Wild ${w.name}`, build: b, palette, creature, home, sight, state: 'wander', t: 0, wanderAt: 0, mark: null } as Wild;
  });
}
spawnWilds();

/** After a fight or a getaway, wild crawlers ignore you for a moment so you can walk off. */
let grace = 2;

function updateWild(w: Wild, dt: number) {
  const c = w.creature;
  const d = distXZ(c.pos, player.pos);
  const spots = grace <= 0 && d < w.sight;
  c.lookPoint = d < w.sight * 1.5 ? player.frame.p : null;
  if (w.state === 'wander' || w.state === 'return') {
    c.mobility = AMBLE;
    if (spots) {
      w.state = 'alert';
      w.t = 0.7;
      w.mark = { text: '!', until: time + 0.9 };
      c.goal = null;
    } else if (w.state === 'return') {
      c.goal = w.home;
      if (distXZ(c.pos, w.home) < 0.6) w.state = 'wander';
    } else if (time > w.wanderAt) {
      const r = Math.random() * 3.5, a = Math.random() * Math.PI * 2;
      const x = w.home[0] + Math.cos(a) * r, z = w.home[2] + Math.sin(a) * r;
      c.goal = [x, world.heightAt(x, z), z];
      w.wanderAt = time + 3 + Math.random() * 3;
    }
  } else if (w.state === 'alert') {
    // Freeze and stare, Pokémon-trainer style, then charge.
    const to = [player.pos[0] - c.pos[0], 0, player.pos[2] - c.pos[2]] as V3;
    c.face = norm(to, [0, 0, 1]);
    w.t -= dt;
    if (w.t <= 0) {
      w.state = 'chase';
      w.t = 9;
      c.face = null;
    }
  } else {
    c.mobility = 1;
    c.goal = [...player.pos] as V3;
    w.t -= dt;
    if (d > w.sight * 1.8 || w.t <= 0 || grace > 0) {
      w.state = 'return';
      w.mark = { text: '?', until: time + 1.2 };
    }
  }
  c.update(dt, time);
}

/** Close enough that the hulls (plus a bit of leg) touch. */
function touching(w: Wild) {
  const reach = (hullDims(player.design).L + hullDims(w.creature.design).L) / 2 * 0.8 + 0.45;
  return distXZ(player.pos, w.creature.pos) < reach;
}

// ---------------------------------------------------------------- scenes and transitions

type Scene = 'roam' | 'battle';
let scene: Scene = 'roam';
let battle: Battle | null = null;
let foe: Wild | null = null;
/** Seconds the fight keeps playing after it's decided, so the knockout or ring-out lands before the result window. */
let resultDelay = 0;

/** A screen transition: closes for `close` seconds, swaps the scene while covered, then opens. */
interface Wipe {
  style: 'encounter' | 'fade';
  t: number;
  close: number;
  open: number;
  swap: () => void;
  swapped: boolean;
}
let wipe: Wipe | null = null;

function transition(style: Wipe['style'], swap: () => void) {
  wipe = style === 'encounter'
    ? { style, t: 0, close: 1.3, open: 0.6, swap, swapped: false }
    : { style, t: 0, close: 0.45, open: 0.45, swap, swapped: false };
}

function encounter(w: Wild) {
  foe = w;
  w.mark = null;
  ui.roam.style.display = 'none';
  transition('encounter', () => {
    scene = 'battle';
    renderer.setMeshes([...prims, pit.mesh]);
    battle = new Battle(pit, { name: 'Your crawler', build, palette: 0 }, { name: w.name, build: w.build, palette: w.palette });
    battle.fighters[0].hp = hp;
    battle.log.unshift(`${w.name} attacks!`);
    resultDelay = 2;
    battleCam();
    ui.fight.style.display = 'block';
    ui.fightmenu.style.display = 'flex';
    ui.result.style.display = 'none';
  });
}

/** Back to the yard. The foe heads home unless it was scrapped. */
function leaveBattle(outcome: 'won' | 'lost' | 'ran' | 'draw') {
  const b = battle!, w = foe!;
  transition('fade', () => {
    scene = 'roam';
    renderer.setMeshes([...prims, world.mesh]);
    if (outcome === 'won') {
      wilds = wilds.filter((x) => x !== w);
      scrapped++;
      if (!wilds.length) spawnWilds();
    } else {
      w.state = 'return';
      w.creature.goal = w.home;
    }
    if (outcome === 'lost') {
      hp = 100;
      player = spawnPlayer();
    } else if (outcome !== 'ran') {
      hp = Math.max(1, Math.round(b.fighters[0].hp));
    }
    grace = outcome === 'ran' ? 4 : 3;
    battle = null;
    foe = null;
    roamCam();
    ui.roam.style.display = 'block';
    ui.fight.style.display = 'none';
  });
}

function outcome(b: Battle): 'won' | 'lost' | 'draw' {
  const r = b.result!;
  return !r.winner ? 'draw' : r.winner === b.fighters[0] ? 'won' : 'lost';
}

function showResult(b: Battle) {
  const o = outcome(b), name = foe!.name;
  const left = wilds.length - 1;
  ui.resultText.textContent = {
    won: `${name} is scrapped for parts. ${left ? `${left} wild crawler${left === 1 ? '' : 's'} still roam the yard.` : 'That was the last one, for now.'}`,
    lost: `Your crawler is towed back to the repair pad and patched up. ${name} wanders off.`,
    draw: `Neither crawler gives in. ${name} wanders off.`,
  }[o];
  ui.fightmenu.style.display = 'none';
  ui.result.style.display = 'flex';
  (document.getElementById('continue') as HTMLButtonElement).focus();
}

document.getElementById('continue')!.addEventListener('click', () => {
  if (battle?.result && !wipe) leaveBattle(outcome(battle));
});
document.getElementById('run')!.addEventListener('click', () => {
  if (battle && !battle.result && !wipe) {
    battle.log.push('You got away safely.');
    leaveBattle('ran');
  }
});

// ---------------------------------------------------------------- camera and input

const cam = new Camera();
const list = new DrawList();
const hud = new FightHud();

function roamCam() {
  cam.pitch = (30 * Math.PI) / 180;
  cam.zoom = 64;
  cam.target = [...player.frame.p] as V3;
}
function battleCam() {
  cam.yaw = cam.targetYaw = 0.5;
  cam.pitch = 0.62;
  cam.zoom = 95;
  cam.target = [0, -0.2, 0];
}
roamCam();

const keys = new Set<string>();
addEventListener('keydown', (e) => {
  const k = e.key.toLowerCase();
  keys.add(k);
  if (scene !== 'roam') return;
  if (k === 'q') cam.targetYaw -= Math.PI / 2;
  if (k === 'e') cam.targetYaw += Math.PI / 2;
});
addEventListener('keyup', (e) => keys.delete(e.key.toLowerCase()));
addEventListener('blur', () => keys.clear());

const dpr = () => window.devicePixelRatio || 1;
let marker: V3 | null = null;
let drag: { x: number; yaw: number } | null = null;
canvas.addEventListener('contextmenu', (e) => e.preventDefault());
canvas.addEventListener('pointerdown', (e) => {
  if (scene !== 'roam' || wipe) return;
  if (e.button === 2) {
    drag = { x: e.clientX, yaw: cam.targetYaw };
    canvas.setPointerCapture(e.pointerId);
    return;
  }
  const { origin, dir } = cam.ray(e.clientX * dpr(), e.clientY * dpr());
  const hit = world.raycast(origin, dir);
  if (hit) player.goal = marker = hit;
});
canvas.addEventListener('pointermove', (e) => {
  if (!drag) return;
  cam.targetYaw = drag.yaw + (e.clientX - drag.x) * 0.01;
  cam.yaw = cam.targetYaw;
});
canvas.addEventListener('pointerup', () => {
  if (drag) cam.targetYaw = Math.round(cam.targetYaw / (Math.PI / 4)) * (Math.PI / 4);
  drag = null;
});
canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  cam.zoom = clamp(cam.zoom * Math.exp(-e.deltaY * 0.0015), 40, 140);
}, { passive: false });

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

// ---------------------------------------------------------------- loop

let time = 0;
let last = performance.now();
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

function updateRoam(dt: number) {
  grace -= dt;
  player.drive = keyDrive();
  if (player.drive) player.goal = marker = null;
  player.update(dt, time);
  if (!player.goal) marker = null;
  for (const w of wilds) updateWild(w, dt);
  const onPad = distXZ(player.pos, PAD) < PAD_RADIUS;
  if (onPad) hp = Math.min(100, hp + 30 * dt);
  const met = grace <= 0 && wilds.find(touching);
  if (met) {
    player.goal = marker = null;
    player.drive = null;
    encounter(met);
  }

  ui.hp.style.width = `${hp}%`;
  ui.line.innerHTML = `${onPad && hp < 100 ? 'Repairing' : 'HP'} <b>${Math.ceil(hp)}</b>` +
    ` · scrapped <b>${scrapped}</b> · ${wilds.length} wild about`;
}

function roamCamera(dt: number) {
  const k = damp(3, dt);
  const p = player.frame.p;
  cam.target = [mix(cam.target[0], p[0], k), mix(cam.target[1], p[1] - 0.3, k), mix(cam.target[2], p[2], k)];
  cam.yaw += wrapAngle(cam.targetYaw - cam.yaw) * damp(8, dt);
}

function battleCamera(b: Battle, dt: number) {
  const [a, c] = b.fighters.map((f) => f.creature.frame.p);
  const mid: V3 = [(a[0] + c[0]) / 2, Math.max(-0.5, (a[1] + c[1]) / 2 - 0.2), (a[2] + c[2]) / 2];
  const k = damp(3, dt);
  cam.target = [mix(cam.target[0], mid[0], k), mix(cam.target[1], mid[1], k), mix(cam.target[2], mid[2], k)];
}

function drawPad(list: DrawList) {
  const [x, y, z] = PAD;
  list.add(Prim.Cyl, [x, y, z], qIdentity(), [PAD_RADIUS * 2 + 0.16, 0.3, PAD_RADIUS * 2 + 0.16], hex('#2f3033'));
  list.add(Prim.Cyl, [x, y + 0.02, z], qIdentity(), [PAD_RADIUS * 2, 0.3, PAD_RADIUS * 2], hex('#d9ae3a'));
  for (const yaw of [0, Math.PI / 2]) list.add(Prim.Box, [x, y + 0.18, z], qAxisAngle([0, 1, 0], yaw), [1.1, 0.02, 0.32], hex('#9cff57'), Mat.Emissive);
  // A lamp post so you can find your way back.
  list.add(Prim.Cyl, [x + 1.4, y + 1, z - 0.6], qIdentity(), [0.1, 2, 0.1], hex('#3b3d42'));
  list.add(Prim.Sphere, [x + 1.4, y + 2.05, z - 0.6], qIdentity(), [0.26, 0.26, 0.26], hex('#9cff57'), Mat.Emissive);
}

function frame(now: number) {
  const dt = Math.min(0.05, (now - last) / 1000);
  last = now;
  time += dt;

  if (wipe) {
    wipe.t += dt;
    if (!wipe.swapped && wipe.t >= wipe.close) {
      wipe.swapped = true;
      wipe.swap();
    }
    if (wipe.t >= wipe.close + wipe.open) wipe = null;
  }
  // The yard freezes while the encounter wipe closes; the fight runs as soon as it opens.
  const frozen = wipe !== null && !wipe.swapped;
  if (scene === 'roam') {
    if (!frozen) updateRoam(dt);
    roamCamera(dt);
  } else if (battle) {
    if (!frozen) battle.update(dt);
    battleCamera(battle, dt);
    if (battle.result && resultDelay > 0 && (resultDelay -= dt) <= 0) showResult(battle);
  }
  resize();
  cam.update(canvas.width, canvas.height, dpr(), settings.pixelSize);

  list.clear();
  if (scene === 'roam') {
    world.draw(list);
    drawPad(list);
    player.draw(list);
    // Wild crawlers off in the distance are off-screen; skip drawing them.
    for (const w of wilds) if (distXZ(w.creature.pos, cam.target) < 20) w.creature.draw(list);
    if (marker) {
      const bob = 0.25 + Math.sin(time * 6) * 0.06;
      list.add(Prim.Cone, add(marker, [0, bob, 0]), qAxisAngle([1, 0, 0], Math.PI), [0.18, 0.25, 0.18], [1, 0.75, 0.3], Mat.Emissive);
    }
  } else if (battle) {
    pit.draw(list);
    battle.draw(list);
  }
  const ly = cam.yaw - Math.PI / 4;
  const lightDir = norm([-0.35 * Math.cos(ly) + 0.55 * Math.sin(ly), 1, -0.35 * -Math.sin(ly) + 0.55 * Math.cos(ly)] as V3);
  renderer.render(cam, list, settings, lightDir, time);

  const g = overlay.getContext('2d')!;
  g.clearRect(0, 0, overlay.width, overlay.height);
  if (scene === 'roam') drawMarks(g);
  else if (battle) {
    hud.drawPopups(g, cam, battle, dpr());
    hud.update(battle);
  }
  if (wipe) drawWipe(g, wipe);
  requestAnimationFrame(frame);
}

/** "!" and "?" bubbles over wild crawlers' heads. */
function drawMarks(g: CanvasRenderingContext2D) {
  const d = dpr();
  g.textAlign = 'center';
  g.font = `bold ${26 * d}px Tahoma, Verdana, sans-serif`;
  for (const w of wilds) {
    if (!w.mark || time > w.mark.until) continue;
    const [x, y] = cam.project(add(w.creature.frame.p, [0, 1.1, 0]));
    const pop = Math.min(1, (w.mark.until - time) * 8);
    const s = 18 * d * (w.mark.text === '!' ? 1 : 0.9);
    g.fillStyle = '#1d1816';
    g.fillRect(x - s - 3 * d, y - s * 2 - 3 * d, s * 2 + 6 * d, s * 2 + 6 * d);
    g.fillStyle = '#fff3c4';
    g.fillRect(x - s, y - s * 2, s * 2, s * 2);
    g.globalAlpha = pop;
    g.fillStyle = w.mark.text === '!' ? '#d94b3b' : '#5f7f86';
    g.fillText(w.mark.text, x, y - s * 0.45);
    g.globalAlpha = 1;
  }
}

/**
 * The encounter wipe: two white flashes, then black bands sweep in from
 * alternate sides in chunky steps, and the battle is revealed by a curtain
 * opening from the middle. Leaving a fight is a plain fade.
 */
function drawWipe(g: CanvasRenderingContext2D, w: Wipe) {
  const W = overlay.width, H = overlay.height;
  const block = 12 * dpr();
  const snap = (v: number) => Math.round(v / block) * block;
  g.fillStyle = '#000';
  if (w.style === 'fade') {
    g.globalAlpha = w.t < w.close ? w.t / w.close : 1 - (w.t - w.close) / w.open;
    g.fillRect(0, 0, W, H);
    g.globalAlpha = 1;
    return;
  }
  if (w.t >= w.close) {
    const half = snap((W / 2) * (1 - (w.t - w.close) / w.open));
    g.fillRect(0, 0, half, H);
    g.fillRect(W - half, 0, half, H);
    return;
  }
  const flashes = 0.45;
  if (w.t < flashes) {
    if (Math.floor(w.t / (flashes / 4)) % 2 === 0) {
      g.fillStyle = '#fff';
      g.globalAlpha = 0.85;
      g.fillRect(0, 0, W, H);
      g.globalAlpha = 1;
    }
    return;
  }
  const p = (w.t - flashes) / (w.close - flashes - 0.1);
  const bands = 10, bh = Math.ceil(H / bands);
  for (let i = 0; i < bands; i++) {
    const k = clamp(p * 1.4 - (i / bands) * 0.4, 0, 1);
    const width = snap(W * k);
    g.fillRect(i % 2 ? W - width : 0, i * bh, width, bh);
  }
}

ui.hint.innerHTML =
  '<b>click</b> walk here  <b>WASD</b> drive  <b>Q/E</b> or right-drag rotate  <b>wheel</b> zoom  ·  ' +
  'wild crawlers that spot you charge; touch one to fight  ·  stand on the green pad to repair';
requestAnimationFrame(frame);
