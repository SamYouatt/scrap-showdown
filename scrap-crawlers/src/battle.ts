// Auto-battle rules for two crawlers in the pit. Each fighter picks a plan from
// its build (ram with a wedge, grab with a pincer, or just bump), and the
// procedural walker does the rest. Win by knocking the other one out or off
// the platform.
import { Creature, hullDims } from './creature';
import { DrawList, Mat } from './render/drawlist';
import { Prim } from './render/mesh';
import { Build, designFor } from './parts';
import { Pit } from './pit';
import { CLAW_REACH, ToolSlot, mountAt } from './tools';
import { V3, add, addScaled, clamp, dist, dot, len, norm, qIdentity, qRotate, scale, sub } from './math';

const COUNTDOWN = 2.5;
const TIME_LIMIT = 75;

type Plan = 'ram' | 'grab' | 'bump' | 'blind' | 'stuck';

interface Weapon {
  plan: Plan;
  /** Pincer mount, if grabbing. */
  slot: ToolSlot | null;
  /** Hull-local direction the weapon faces. */
  aim: V3;
}

export class Fighter {
  hp = 100;
  out = false;
  cooldown = 0;
  backoff = 0;
  holding: Fighter | null = null;
  heldBy: Fighter | null = null;
  holdLeft = 0;
  /** Which way to veer when backing off after a hit, so rematches play out differently. */
  dodge = 1;
  readonly weapon: Weapon;
  /** Solid footprint in hull space: hull plus whatever sticks out of it. */
  readonly shape: Shape;
  private wanderAt = 0;

  constructor(readonly name: string, readonly build: Build, readonly creature: Creature) {
    const d = creature.design;
    this.shape = footprint(creature);
    this.weapon = pickWeapon(creature);
    if (!d.legPairs) creature.mobility = 0;
    creature.toolkit.manual = true;
  }

  get alive() {
    return this.hp > 0 && !this.out;
  }

  get plan(): string {
    const w = this.weapon;
    if (w.plan === 'grab' && w.aim[2] < 0) return 'grabs (backwards)';
    return { ram: 'rams', grab: 'grabs', bump: 'bumps', blind: 'blind', stuck: 'stuck' }[w.plan];
  }

  /** World direction the weapon points. */
  aimWorld(): V3 {
    return qRotate(this.creature.frame.q, this.weapon.aim);
  }

  think(o: Fighter, pit: Pit, time: number, dt: number) {
    const c = this.creature, oc = o.creature;
    this.cooldown -= dt;
    this.backoff -= dt;
    const toO: V3 = [oc.pos[0] - c.pos[0], 0, oc.pos[2] - c.pos[2]];
    const dir = norm(toO, [1, 0, 0]);
    c.lookPoint = add(oc.frame.p, [0, 0.2, 0]);
    if (this.weapon.plan === 'stuck') return;

    if (this.weapon.plan === 'blind') {
      // No control module: wander about and hope.
      c.face = null;
      if (time > this.wanderAt) {
        this.wanderAt = time + 2 + Math.random() * 2;
        c.goal = keepOn(pit, [(Math.random() - 0.5) * 8, 0, (Math.random() - 0.5) * 8]);
      }
      return;
    }

    // Turn so the weapon, not necessarily the nose, points at the opponent.
    const a = this.weapon.aim;
    const yaw = Math.atan2(dir[0], dir[2]) - Math.atan2(a[0], a[2]);
    c.face = [Math.sin(yaw), 0, Math.cos(yaw)];

    if (this.holding) {
      c.goal = null;
      return;
    }
    const side: V3 = [-dir[2] * this.dodge, 0, dir[0] * this.dodge];
    if (this.backoff > 0) {
      c.goal = keepOn(pit, addScaled(addScaled(c.pos, dir, -1.6), side, 1.2));
    } else if (this.weapon.plan === 'grab') {
      // Close to claw range, then hold position.
      const reach = dot(mountAt(hullDims(c.design), this.weapon.slot!).p, norm(a)) + CLAW_REACH;
      c.goal = keepOn(pit, addScaled(oc.pos, dir, -(reach + 0.1)));
    } else {
      // Charge through where the opponent is heading.
      c.goal = keepOn(pit, addScaled(addScaled(oc.pos, oc.vel, 0.3), dir, 0.8));
    }
  }
}

function pickWeapon(c: Creature): Weapon {
  const d = c.design;
  if (!d.legPairs) return { plan: 'stuck', slot: null, aim: [0, 0, 1] };
  if (!d.head) return { plan: 'blind', slot: null, aim: [0, 0, 1] };
  const tk = c.toolkit;
  if (tk.powered && tk.pincers.length) {
    const slot = tk.pincers[0];
    return { plan: 'grab', slot, aim: mountAt(hullDims(c.design), slot).aim };
  }
  if (d.tools.front === 'wedge') return { plan: 'ram', slot: null, aim: [0, 0, 1] };
  return { plan: 'bump', slot: null, aim: [0, 0, 1] };
}

/** Keeps a goal far enough inside the platform that the AI never walks off on its own. */
function keepOn(pit: Pit, p: V3): V3 {
  const lim = pit.size - 1;
  return [clamp(p[0], -lim, lim), 0, clamp(p[2], -lim, lim)];
}

/** A fighter's footprint: half-width, and how far it reaches forwards and backwards from its centre. */
interface Shape {
  side: number;
  front: number;
  back: number;
}

/** The hull box grown to cover the tools and head fitted to it. Legs are left out: their feet step aside instead. */
function footprint(c: Creature): Shape {
  const d = c.design, { L, W } = hullDims(d);
  const s: Shape = { side: W / 2 + 0.08, front: L / 2 + 0.05, back: L / 2 + 0.05 };
  const t = d.tools;
  if (t.front === 'wedge') s.front = L / 2 + 0.5;
  if (t.front === 'pincer') s.front = L / 2 + 0.4;
  if (t.fronttop === 'pincer') s.front = Math.max(s.front, L / 2 + 0.3);
  if (t.back === 'pincer') s.back = L / 2 + 0.3;
  if (d.head && d.headSlot === 'fronttop') s.front = Math.max(s.front, L / 2 + 0.3);
  if (d.head && d.headSlot === 'back') s.back = Math.max(s.back, L / 2 + 0.2);
  if (d.head && d.headSlot === 'front') s.front = Math.max(s.front, L / 2 + 0.35);
  return s;
}

/** The footprint as a centred box on the ground: centre, forward and side axes, and half extents. */
function box(f: Fighter) {
  const c = f.creature, s = f.shape;
  const fwd: V3 = [Math.sin(c.yaw), 0, Math.cos(c.yaw)];
  const side: V3 = [fwd[2], 0, -fwd[0]];
  const centre = addScaled([c.pos[0], 0, c.pos[2]], fwd, (s.front - s.back) / 2);
  return { centre, fwd, side, hz: (s.front + s.back) / 2, hx: s.side };
}

/** How far the footprint reaches from the fighter's position along a ground direction. */
function reachAlong(f: Fighter, dir: V3) {
  const b = box(f);
  const centreShift = dot(sub(b.centre, [f.creature.pos[0], 0, f.creature.pos[2]]), dir);
  return Math.abs(dot(dir, b.fwd)) * b.hz + Math.abs(dot(dir, b.side)) * b.hx + centreShift;
}

/** Separating-axis test of two footprints. Returns how far to push b along n (from a to b) to clear a. */
function overlap(a: Fighter, b: Fighter): { n: V3; depth: number } | null {
  const A = box(a), B = box(b);
  const d = sub(B.centre, A.centre);
  let best: { n: V3; depth: number } | null = null;
  for (const axis of [A.fwd, A.side, B.fwd, B.side]) {
    const ra = Math.abs(dot(axis, A.fwd)) * A.hz + Math.abs(dot(axis, A.side)) * A.hx;
    const rb = Math.abs(dot(axis, B.fwd)) * B.hz + Math.abs(dot(axis, B.side)) * B.hx;
    const dist = dot(d, axis);
    const depth = ra + rb - Math.abs(dist);
    if (depth <= 0) return null;
    if (!best || depth < best.depth) best = { n: scale(axis, dist < 0 ? -1 : 1), depth };
  }
  return best;
}

/** Moves a ground point out of a fighter's footprint (plus a margin), to its nearest edge. */
function pushOut(f: Fighter, p: V3, margin: number): V3 {
  if (f.out) return p;
  const b = box(f);
  const rel = sub([p[0], 0, p[2]], b.centre);
  const z = dot(rel, b.fwd), x = dot(rel, b.side);
  const hz = b.hz + margin, hx = b.hx + margin;
  const dz = hz - Math.abs(z), dx = hx - Math.abs(x);
  if (dz <= 0 || dx <= 0) return p;
  const q = dz < dx ? addScaled(p, b.fwd, (z < 0 ? -1 : 1) * dz) : addScaled(p, b.side, (x < 0 ? -1 : 1) * dx);
  return [q[0], p[1], q[2]];
}

/** Keeps a foothold off another fighter: outside its footprint and away from its feet. */
function clearOf(o: Fighter, p: V3): V3 {
  if (o.out) return p;
  let q = pushOut(o, p, 0.12);
  for (const l of o.creature.legs) {
    const d: V3 = [q[0] - l.foot[0], 0, q[2] - l.foot[2]];
    const gap = len(d);
    if (gap < FOOT_GAP) q = addScaled(q, norm(d, [1, 0, 0]), FOOT_GAP - gap);
  }
  return q[0] === p[0] && q[2] === p[2] ? p : q;
}

/** How close a foot may land to the other fighter's feet. */
const FOOT_GAP = 0.35;

interface Spark {
  p: V3;
  v: V3;
  life: number;
}

export interface Popup {
  p: V3;
  text: string;
  age: number;
  hot: boolean;
}

/** A fighter's name and build; palette defaults to orange for the first, teal for the second. */
export interface Entrant {
  name: string;
  build: Build;
  palette?: number;
}

export class Battle {
  readonly fighters: [Fighter, Fighter];
  time = 0;
  result: { winner: Fighter | null; reason: string } | null = null;
  log: string[] = [];
  popups: Popup[] = [];
  private sparks: Spark[] = [];

  constructor(readonly pit: Pit, a: Entrant, b: Entrant) {
    const make = (f: Entrant, x: number, palette: number) => {
      const design = { ...designFor(f.build), palette: f.palette ?? palette };
      return new Fighter(f.name, f.build, new Creature(design, pit, [x, 0, 0], x < 0 ? Math.PI / 2 : -Math.PI / 2));
    };
    this.fighters = [make(a, -3, 0), make(b, 3, 1)];
    for (const f of this.fighters) this.note(`${f.name} ${f.plan}.`);
  }

  get countdown() {
    return Math.max(0, COUNTDOWN - this.time);
  }

  get timeLeft() {
    return Math.max(0, COUNTDOWN + TIME_LIMIT - this.time);
  }

  private note(msg: string) {
    this.log.push(msg);
    if (this.log.length > 6) this.log.shift();
  }

  update(dt: number) {
    this.time += dt;
    const [a, b] = this.fighters;
    const live = this.time > COUNTDOWN && !this.result;
    for (const [f, o] of [[a, b], [b, a]] as const) {
      const c = f.creature;
      if (!f.alive) {
        c.goal = null;
        c.face = null;
        c.down = f.hp <= 0;
      } else if (live) {
        f.think(o, this.pit, this.time, dt);
      } else {
        c.lookPoint = o.creature.frame.p;
      }
      // Feet never plant inside the other fighter.
      c.avoid = (p) => clearOf(o, p);
      c.update(dt, this.time);
    }
    if (live) {
      for (const [f, o] of [[a, b], [b, a]] as const) {
        this.strike(f, o);
        this.hold(f, o, dt);
      }
      this.ringOut();
      this.judge();
    }
    // Solid contact goes last so nothing (a grab's pull included) can leave them overlapping.
    for (let i = 0; i < 4; i++) this.collide(a, b);
    this.updateEffects(dt);
  }

  /** Footprints are boxes on the ground; overlapping ones are pushed apart and stop closing. */
  private collide(a: Fighter, b: Fighter) {
    if (a.out || b.out) return;
    const hit = overlap(a, b);
    if (!hit) return;
    const ca = a.creature, cb = b.creature;
    // A held fighter can't brace, so it takes the whole push.
    const share = b.heldBy === a ? 1 : a.heldBy === b ? 0 : 0.5;
    ca.pos = addScaled(ca.pos, hit.n, -hit.depth * (1 - share));
    cb.pos = addScaled(cb.pos, hit.n, hit.depth * share);
    for (const [c, s] of [[ca, -1], [cb, 1]] as const) {
      const into = dot(c.vel, hit.n) * s;
      if (into < 0) c.vel = addScaled(c.vel, hit.n, -into * s);
      const pin = dot(c.push, hit.n) * s;
      if (pin < 0) c.push = addScaled(c.push, hit.n, -pin * s);
    }
  }

  /** Rams and bumps: the weapon face has to be pointing at the target and closing fast. */
  private strike(f: Fighter, o: Fighter) {
    if (!f.alive || !o.alive || f.cooldown > 0 || f.heldBy) return;
    const c = f.creature, oc = o.creature;
    const toO = sub([oc.pos[0], 0, oc.pos[2]], [c.pos[0], 0, c.pos[2]]);
    const gap = len(toO), dir = norm(toO, [1, 0, 0]);
    const facing = dot(norm(f.aimWorld()), dir);
    const plan = f.weapon.plan;

    if (plan === 'grab') {
      if (f.holding || o.heldBy || facing < 0.75) return;
      const tip = c.toolkit.clawTip(c.frame, f.weapon.slot!);
      if (dist(tip, pushOut(o, tip, 0)) > 0.3) return;
      c.toolkit.snap(f.weapon.slot!);
      c.toolkit.grip(f.weapon.slot!, true);
      f.holding = o;
      o.heldBy = f;
      f.holdLeft = 2.2;
      oc.mobility = 0.15;
      this.hit(o, 6, tip, `${f.name} grabs ${o.name}!`);
      return;
    }

    const closing = dot(c.vel, dir);
    if (gap - reachAlong(f, dir) - reachAlong(o, scale(dir, -1)) > 0.2 || facing < 0.6 || closing < 0.35) return;
    const ram = plan === 'ram';
    const dmg = ram ? 6 + closing * 7 : 2 + closing * 2;
    oc.push = addScaled(oc.push, dir, ram ? 2.6 + closing * 1.8 : 1.2);
    c.push = addScaled(c.push, dir, -0.6);
    f.cooldown = ram ? 0.9 : 0.8;
    f.backoff = ram ? 0.6 + Math.random() * 0.3 : 0.6;
    f.dodge = Math.random() < 0.5 ? 1 : -1;
    this.hit(o, dmg, addScaled(c.pos, dir, reachAlong(f, dir)), ram ? `${f.name} rams ${o.name}` : `${f.name} bumps ${o.name}`);
  }

  /** A held fighter is dragged to the claw and squeezed, then flung away on release. */
  private hold(f: Fighter, o: Fighter, dt: number) {
    if (!f.holding) return;
    const c = f.creature, oc = o.creature, slot = f.weapon.slot!;
    f.holdLeft -= dt;
    // The jaws hold the near edge of the opponent, so its centre sits beyond the tip.
    const aim = norm([f.aimWorld()[0], 0, f.aimWorld()[2]]);
    // Its near face, which points back along the claw, meets the jaws.
    const tip = addScaled(c.toolkit.clawTip(c.frame, slot), aim, reachAlong(o, scale(aim, -1)));
    const pull = 1 - Math.exp(-5 * dt);
    oc.pos = [oc.pos[0] + (tip[0] - oc.pos[0]) * pull, oc.pos[1], oc.pos[2] + (tip[2] - oc.pos[2]) * pull];
    o.hp -= 8 * dt;
    if (f.holdLeft > 0 && f.alive && o.alive) return;
    c.toolkit.grip(slot, false);
    if (o.alive) {
      oc.push = addScaled(oc.push, norm(f.aimWorld()), 3.2);
      this.note(`${f.name} flings ${o.name}`);
    }
    oc.mobility = 1;
    f.holding = null;
    o.heldBy = null;
    f.cooldown = 1.6;
    f.backoff = 0.7 + Math.random() * 0.4;
    f.dodge = Math.random() < 0.5 ? 1 : -1;
  }

  private hit(o: Fighter, dmg: number, at: V3, msg: string) {
    o.hp = Math.max(0, o.hp - dmg);
    this.note(`${msg} (−${Math.round(dmg)})`);
    this.popups.push({ p: add(at, [0, 0.9, 0]), text: `-${Math.round(dmg)}`, age: 0, hot: dmg >= 12 });
    for (let i = 0; i < 10; i++) {
      const v: V3 = [(Math.random() - 0.5) * 4, 1 + Math.random() * 2.5, (Math.random() - 0.5) * 4];
      this.sparks.push({ p: add(at, [0, 0.35, 0]), v, life: 0.25 + Math.random() * 0.3 });
    }
  }

  private ringOut() {
    for (const f of this.fighters) {
      const p = f.creature.pos;
      if (f.out || this.pit.onPlatform(p[0], p[2], 0.15)) continue;
      f.out = true;
      if (f.heldBy) {
        f.heldBy.holding = null;
        f.heldBy.creature.toolkit.grip(f.heldBy.weapon.slot!, false);
        f.heldBy = null;
      }
      this.note(`${f.name} goes over the edge!`);
    }
  }

  private judge() {
    const [a, b] = this.fighters;
    const why = (f: Fighter) => (f.out ? 'ring out' : 'knockout');
    if (!a.alive && !b.alive) this.result = { winner: null, reason: 'double knockout' };
    else if (!a.alive) this.result = { winner: b, reason: why(a) };
    else if (!b.alive) this.result = { winner: a, reason: why(b) };
    else if (this.time > COUNTDOWN + TIME_LIMIT) {
      this.result = a.hp === b.hp ? { winner: null, reason: 'time' } : { winner: a.hp > b.hp ? a : b, reason: 'on points' };
    }
    if (this.result) this.note(this.result.winner ? `${this.result.winner.name} wins by ${this.result.reason}.` : `Draw (${this.result.reason}).`);
  }

  private updateEffects(dt: number) {
    for (const s of this.sparks) {
      s.life -= dt;
      s.v[1] -= 9 * dt;
      s.p = addScaled(s.p, s.v, dt);
    }
    this.sparks = this.sparks.filter((s) => s.life > 0);
    for (const p of this.popups) {
      p.age += dt;
      p.p = add(p.p, [0, dt * 0.8, 0]);
    }
    this.popups = this.popups.filter((p) => p.age < 1.1);
  }

  draw(list: DrawList) {
    for (const f of this.fighters) f.creature.draw(list);
    for (const s of this.sparks) {
      const k = Math.min(1, s.life * 4);
      list.add(Prim.Box, s.p, qIdentity(), scale([0.07, 0.07, 0.07], k), [1, 0.75 + k * 0.2, 0.3], Mat.Emissive);
    }
  }
}
