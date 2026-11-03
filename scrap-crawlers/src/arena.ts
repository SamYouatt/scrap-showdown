// The arena: your build (from the workshop) against a rival, fought out
// automatically in the pit.
import { Camera } from './render/camera';
import { DrawList } from './render/drawlist';
import { primitiveMeshes } from './render/mesh';
import { Renderer } from './render/renderer';
import { Battle } from './battle';
import { Pit } from './pit';
import { decodeBuild } from './parts';
import { V3, clamp, mix, norm } from './math';

const RIVALS = [
  { name: 'Plowboy', build: 'box;legs=quad;fronttop=can;front=wedge' },
  { name: 'Snapper', build: 'box;legs=crab;fronttop=tv;front=pincer;spine=pneumatics' },
  { name: 'Backhand', build: 'box;legs=millipede;fronttop=lamp;back=pincer;spine=pneumatics' },
  { name: 'Skitter', build: 'box;legs=strider;fronttop=tv;front=wedge' },
];

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('gpu') as HTMLCanvasElement;
const overlay = document.getElementById('overlay') as HTMLCanvasElement;
const banner = document.getElementById('banner') as HTMLDivElement;
const logEl = document.querySelector('#log .sunken') as HTMLDivElement;
const clock = document.getElementById('clock') as HTMLDivElement;

let renderer: Renderer;
try {
  renderer = await Renderer.create(canvas);
} catch (e) {
  (document.getElementById('nogpu') as HTMLDivElement).style.display = 'grid';
  (document.getElementById('nogpu-msg') as HTMLParagraphElement).textContent = String((e as Error).message ?? e);
  throw e;
}

const pit = new Pit();
renderer.setMeshes([...primitiveMeshes(), pit.mesh]);
const settings = { pixelSize: Number(params.get('pixel') ?? 3), outlines: true, palette: false, shadows: true };

// Your build comes from the workshop; without one you watch two rivals.
const yours = params.get('a');
let rival = Number(params.get('rival') ?? (yours ? 0 : 1)) % RIVALS.length;
const player = yours ? { name: 'Your build', build: decodeBuild(yours) } : { name: RIVALS[0].name, build: decodeBuild(RIVALS[0].build) };
if (yours) (document.getElementById('back') as HTMLAnchorElement).href = `./workshop.html?build=${encodeURIComponent(yours)}`;

let battle: Battle;
function start() {
  const r = RIVALS[rival];
  battle = new Battle(pit, player, { name: r.name, build: decodeBuild(r.build) });
  const url = new URL(location.href);
  url.searchParams.set('rival', String(rival));
  history.replaceState(null, '', url);
}
start();
document.getElementById('rematch')!.addEventListener('click', start);
document.getElementById('next')!.addEventListener('click', () => {
  rival = (rival + 1) % RIVALS.length;
  if (!yours && rival === 0) rival = 1;
  start();
});

const cam = new Camera();
cam.yaw = cam.targetYaw = 0.5;
cam.pitch = 0.62;
cam.zoom = 95;
const list = new DrawList();

// ---------------------------------------------------------------- input

const dpr = () => window.devicePixelRatio || 1;
let orbit: { x: number; y: number; yaw: number; pitch: number } | null = null;
canvas.addEventListener('contextmenu', (e) => e.preventDefault());
canvas.addEventListener('pointerdown', (e) => (orbit = { x: e.clientX, y: e.clientY, yaw: cam.yaw, pitch: cam.pitch }));
addEventListener('pointermove', (e) => {
  if (!orbit) return;
  cam.yaw = cam.targetYaw = orbit.yaw + (e.clientX - orbit.x) * 0.01;
  cam.pitch = clamp(orbit.pitch + (e.clientY - orbit.y) * 0.006, 0.35, 1.2);
});
addEventListener('pointerup', () => (orbit = null));
canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  cam.zoom = clamp(cam.zoom * Math.exp(-e.deltaY * 0.0015), 60, 220);
}, { passive: false });

// ---------------------------------------------------------------- HUD

function plate(id: string, i: 0 | 1) {
  const f = battle.fighters[i];
  const el = document.getElementById(id)!;
  (el.querySelector('.name') as HTMLDivElement).textContent = f.name;
  (el.querySelector('.plan') as HTMLDivElement).textContent = f.out ? 'out of the ring' : f.hp <= 0 ? 'knocked out' : f.plan;
  (el.querySelector('.bar div') as HTMLDivElement).style.width = `${Math.max(0, f.hp)}%`;
}

let lastBanner = '';
function hud() {
  plate('pa', 0);
  plate('pb', 1);
  const t = battle.countdown;
  clock.textContent = t > 0 ? '' : String(Math.ceil(battle.timeLeft));
  let html = '', cls = 'window';
  if (battle.result) {
    const r = battle.result;
    html = `<div class="titlebar">Fight over</div><div class="wbody"><h1>${r.winner ? `${r.winner.name} wins` : 'Draw'}</h1>` +
      `<div class="why">by ${r.reason}</div></div>`;
  } else if (t > 0) {
    html = `<h1>${Math.ceil(t)}</h1>`;
    cls = 'window count';
  } else if (battle.countdown === 0 && battle.time < 3.3) {
    html = '<h1>CRAWL!</h1>';
    cls = 'window count';
  }
  if (html !== lastBanner) {
    lastBanner = html;
    banner.innerHTML = html;
    banner.className = cls;
    banner.style.display = html ? 'flex' : 'none';
  }
  logEl.innerHTML = battle.log.map((l, i) => (i === battle.log.length - 1 ? `<b>${l}</b>` : l)).join('\n');
}

// ---------------------------------------------------------------- loop

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

function frame(now: number) {
  const dt = Math.min(0.05, (now - last) / 1000);
  last = now;
  battle.update(dt);

  // Keep both fighters in view, following the action on the deck.
  const [a, b] = battle.fighters.map((f) => f.creature.frame.p);
  const mid: V3 = [(a[0] + b[0]) / 2, Math.max(-0.5, (a[1] + b[1]) / 2 - 0.2), (a[2] + b[2]) / 2];
  const k = 1 - Math.exp(-3 * dt);
  cam.target = [mix(cam.target[0], mid[0], k), mix(cam.target[1], mid[1], k), mix(cam.target[2], mid[2], k)];
  resize();
  cam.update(canvas.width, canvas.height, dpr(), settings.pixelSize);

  list.clear();
  pit.draw(list);
  battle.draw(list);
  const ly = cam.yaw - Math.PI / 4;
  const lightDir = norm([-0.35 * Math.cos(ly) + 0.55 * Math.sin(ly), 1, -0.35 * -Math.sin(ly) + 0.55 * Math.cos(ly)] as V3);
  renderer.render(cam, list, settings, lightDir, battle.time);
  drawPopups();
  hud();
  requestAnimationFrame(frame);
}

/** Damage numbers float up from each hit. */
function drawPopups() {
  const g = overlay.getContext('2d')!;
  g.clearRect(0, 0, overlay.width, overlay.height);
  const d = dpr();
  g.font = `bold ${16 * d}px ui-monospace, Menlo, monospace`;
  g.textAlign = 'center';
  for (const p of battle.popups) {
    const [x, y] = cam.project(p.p);
    g.globalAlpha = Math.min(1, (1.1 - p.age) * 3);
    g.fillStyle = '#1d1918';
    g.fillText(p.text, x + 2 * d, y + 2 * d);
    g.fillStyle = p.hot ? '#ffd23a' : '#f2e6cf';
    g.fillText(p.text, x, y);
  }
  g.globalAlpha = 1;
}

requestAnimationFrame(frame);
