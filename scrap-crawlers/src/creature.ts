// A procedurally animated scrap creature. Nothing here is keyframed:
//  - each foot stays planted until the body drifts too far from its ideal spot,
//    then it steps along an arc to a predicted landing point (gait rules decide
//    which legs may lift together)
//  - legs are FABRIK chains from hip to foot, bent towards a pole above and outside
//  - the body floats on a spring above the planted feet and tilts to their plane
//  - the head is a damped spring, the tail follows the leader, antennae are verlet
import { DrawList, Mat } from './render/drawlist';
import { Prim } from './render/mesh';
import {
  Frame, Quat, Rng, V3, add, addScaled, clamp, copy, cross, damp, dist, distXZ, dot, hex, len, lerp, mix,
  norm, qAlongY, qAxisAngle, qIdentity, qLook, qRotate, rng, scale, smoothstep, sub, toWorld, wrapAngle,
} from './math';
import { bentChain, fabrik } from './ik';
import { World } from './world';

export type Gait = 'trot' | 'creep' | 'scurry';
export type LegStyle = 'pipe' | 'girder' | 'piston';
export type HeadStyle = 'can' | 'tv' | 'lamp';
export type Hull = 'crate' | 'drum' | 'pod';

export interface Design {
  seed: number;
  palette: number;
  hull: Hull;
  bodyLength: number;
  bodyWidth: number;
  legPairs: number;
  legSegments: number;
  legLength: number;
  legSpread: number;
  legStyle: LegStyle;
  rideHeight: number;
  stepHeight: number;
  stepSpeed: number;
  walkSpeed: number;
  gait: Gait;
  tail: number;
  antennae: number;
  head: HeadStyle;
}

export const defaultDesign = (): Design => ({
  seed: 7, palette: 0, hull: 'crate', bodyLength: 1.3, bodyWidth: 0.8,
  legPairs: 3, legSegments: 3, legLength: 1.1, legSpread: 1, legStyle: 'pipe',
  rideHeight: 0.45, stepHeight: 0.3, stepSpeed: 1, walkSpeed: 1.6, gait: 'trot',
  tail: 6, antennae: 2, head: 'can',
});

export function randomDesign(seed: number): Design {
  const r = rng(seed);
  const round = (x: number, step: number) => Math.round(x / step) * step;
  return {
    seed, palette: r.int(0, PALETTES.length - 1), hull: r.pick(['crate', 'drum', 'pod'] as const),
    bodyLength: round(r.range(0.8, 2), 0.05), bodyWidth: round(r.range(0.5, 1.1), 0.05),
    legPairs: r.int(1, 4), legSegments: r.int(2, 4), legLength: round(r.range(0.75, 1.6), 0.05),
    legSpread: round(r.range(0.7, 1.4), 0.05), legStyle: r.pick(['pipe', 'girder', 'piston'] as const),
    rideHeight: round(r.range(0.25, 0.7), 0.05), stepHeight: round(r.range(0.15, 0.5), 0.05),
    stepSpeed: round(r.range(0.7, 1.6), 0.05), walkSpeed: round(r.range(1, 2.4), 0.1),
    gait: r.pick(['trot', 'creep', 'scurry'] as const), tail: r.int(0, 10), antennae: r.int(0, 2),
    head: r.pick(['can', 'tv', 'lamp'] as const),
  };
}

interface Palette {
  name: string;
  main: V3;
  second: V3;
  accent: V3;
  dark: V3;
  glow: V3;
}
const pal = (name: string, main: string, second: string, accent: string, dark: string, glow: string): Palette => ({
  name, main: hex(main), second: hex(second), accent: hex(accent), dark: hex(dark), glow: hex(glow),
});
export const PALETTES: Palette[] = [
  pal('Rust bucket', '#b5532c', '#9a9e98', '#e0a040', '#3b3533', '#7ff5ff'),
  pal('Teal tin', '#4f8f8a', '#c9b48a', '#d9603b', '#2e3a3d', '#ffdd55'),
  pal('Hazard', '#e3b23c', '#55555e', '#d94b3b', '#26252a', '#9cff57'),
  pal('Army surplus', '#6f7a3e', '#a59e86', '#c45a2c', '#2f2e26', '#ff6a3d'),
  pal('Plum', '#7b4b6b', '#b9b0a0', '#5fb0a6', '#2b2230', '#ffe08a'),
];

interface Part {
  prim: Prim;
  p: V3;
  q: Quat;
  s: V3;
  col: V3;
  mat?: Mat;
}

export interface Leg {
  side: number;
  pair: number;
  hip: V3;
  rest: V3;
  lengths: number[];
  joints: V3[];
  foot: V3;
  from: V3;
  desired: V3;
  t: number;
  stepping: boolean;
}

interface Dust {
  p: V3;
  v: V3;
  life: number;
  max: number;
  size: number;
}

const UP: V3 = [0, 1, 0];
const X_TO_Z = qAxisAngle([1, 0, 0], Math.PI / 2);

export class Creature {
  readonly design: Design;
  readonly pal: Palette;
  pos: V3;
  yaw: number;
  vel: V3 = [0, 0, 0];
  yawRate = 0;
  frame: Frame;
  legs: Leg[] = [];
  /** Where to walk (click) or which way to walk (keys); keys win. */
  goal: V3 | null = null;
  drive: V3 | null = null;
  lookPoint: V3 | null = null;
  stride = 0.4;

  private readonly L: number;
  private readonly W: number;
  private readonly H: number;
  private readonly hs: number;
  private readonly th: number;
  private readonly parts: Part[];
  private up: V3 = [0, 1, 0];
  private bodyY: number;
  private bodyVy = 0;
  private headPos: V3;
  private headVel: V3 = [0, 0, 0];
  private headDir: V3;
  private tail: V3[] = [];
  private antennae: { base: V3; pts: V3[]; prev: V3[] }[] = [];
  private dust: Dust[] = [];
  private blink = 0;

  constructor(design: Design, private world: World, pos: V3, yaw = 0) {
    this.design = design;
    this.pal = PALETTES[design.palette % PALETTES.length];
    this.pos = [pos[0], 0, pos[2]];
    this.yaw = yaw;
    this.L = design.bodyLength;
    this.W = design.bodyWidth;
    this.H = design.bodyWidth * (design.hull === 'pod' ? 0.7 : 0.55);
    this.hs = 0.26 + this.W * 0.14;
    this.th = 0.07 * Math.sqrt(design.legLength) + 0.025;
    this.stride = Math.max(0.25, design.legLength * 0.42 * design.legSpread);
    this.parts = buildChassis(design, rng(design.seed), this.pal, this.L, this.W, this.H);

    const ground = world.heightAt(pos[0], pos[2]);
    this.bodyY = ground + design.rideHeight * design.legLength + this.H * 0.3;
    this.frame = { p: [pos[0], this.bodyY, pos[2]], q: qAxisAngle(UP, yaw) };

    const weights: Record<number, number[]> = { 2: [0.48, 0.52], 3: [0.32, 0.36, 0.32], 4: [0.24, 0.26, 0.26, 0.24] };
    const lengths = weights[design.legSegments].map((w) => w * design.legLength);
    const pairs = design.legPairs;
    for (let pair = 0; pair < pairs; pair++) {
      const f = pairs > 1 ? pair / (pairs - 1) : 0.5;
      const hipZ = mix(this.L * 0.38, -this.L * 0.38, f);
      const fan = pairs > 1 ? (0.5 - f) * design.legLength * 0.55 * design.legSpread : 0;
      for (const side of [1, -1]) {
        const hip: V3 = [side * this.W * 0.47, -this.H * 0.15, hipZ];
        // Bipeds stand with feet under them; more legs sprawl outwards.
        const reach = pairs === 1 ? 0.22 : 0.6;
        const rest: V3 = [side * (this.W * 0.5 + design.legLength * reach * design.legSpread), 0, hipZ * 1.1 + fan];
        const foot = this.restWorld(rest, 0);
        this.legs.push({
          side, pair, hip, rest, lengths, joints: [], foot, from: copy(foot), desired: copy(foot), t: 1, stepping: false,
        });
      }
    }

    this.headPos = toWorld(this.frame, this.headAnchor());
    this.headDir = qRotate(this.frame.q, [0, 0, 1]);
    for (let i = 0; i <= design.tail; i++) this.tail.push(toWorld(this.frame, [0, 0, -this.L / 2 - i * 0.14]));
    for (let i = 0; i < design.antennae; i++) {
      const base: V3 = [design.antennae === 1 ? 0 : (i ? -1 : 1) * this.hs * 0.3, this.hs * 0.55, -this.hs * 0.1];
      const start = toWorld({ p: this.headPos, q: this.frame.q }, base);
      const pts = Array.from({ length: 5 }, (_, k) => add(start, [0, k * 0.11, 0]));
      this.antennae.push({ base, pts, prev: pts.map(copy) });
    }
    this.solveLegs();
  }

  private stepDuration() {
    const speed = len(this.vel);
    return (0.3 * Math.sqrt(this.design.legLength)) / this.design.stepSpeed / (1 + speed * 0.3);
  }

  private restWorld(rest: V3, lookAhead: number): V3 {
    const yaw = this.yaw + this.yawRate * lookAhead;
    const r = qRotate(qAxisAngle(UP, yaw), [rest[0], 0, rest[2]]);
    const x = this.pos[0] + this.vel[0] * lookAhead + r[0];
    const z = this.pos[2] + this.vel[2] * lookAhead + r[2];
    return [x, this.world.heightAt(x, z), z];
  }

  private headAnchor(): V3 {
    return [0, this.H * 0.45 + this.hs * 0.55, this.L / 2 + this.hs * 0.25];
  }

  update(dt: number, time: number) {
    this.move(dt);
    this.updateBody(dt);
    this.updateLegs(dt);
    this.solveLegs();
    this.updateHead(dt, time);
    this.updateTail(dt, time);
    this.updateAntennae(dt);
    this.updateDust(dt);
  }

  private move(dt: number) {
    const d = this.design;
    let wish: V3 = [0, 0, 0];
    if (this.drive) {
      wish = scale(norm(this.drive), d.walkSpeed);
    } else if (this.goal) {
      const to: V3 = [this.goal[0] - this.pos[0], 0, this.goal[2] - this.pos[2]];
      const dd = len(to);
      if (dd < 0.25) this.goal = null;
      else wish = scale(to, (d.walkSpeed * Math.min(1, dd / 1.2)) / dd);
    }
    // Turn towards where we want to go, and don't charge sideways while turning.
    let yawRate = 0;
    if (len(wish) > 0.05) {
      const want = Math.atan2(wish[0], wish[2]);
      const diff = wrapAngle(want - this.yaw);
      yawRate = clamp(diff * 4, -2.6, 2.6);
      wish = scale(wish, Math.max(0.15, Math.cos(diff)));
    }
    this.yawRate = mix(this.yawRate, yawRate, damp(8, dt));
    this.yaw = wrapAngle(this.yaw + this.yawRate * dt);
    this.vel = lerp(this.vel, wish, damp(3, dt));
    const next = addScaled(this.pos, this.vel, dt);
    const lim = this.world.half - 2;
    next[0] = clamp(next[0], -lim, lim);
    next[2] = clamp(next[2], -lim, lim);
    // Refuse to walk into walls taller than the legs can climb.
    const rise = this.world.heightAt(next[0], next[2]) - this.world.heightAt(this.pos[0], this.pos[2]);
    if (rise > this.design.legLength * 0.75) this.vel = [0, 0, 0];
    else this.pos = next;
  }

  private updateBody(dt: number) {
    const d = this.design;
    let sum = 0;
    for (const l of this.legs) sum += l.stepping ? mix(l.from[1], l.desired[1], 0.5) : l.foot[1];
    const feetY = sum / this.legs.length;
    const floor = this.world.heightAt(this.pos[0], this.pos[2]) + this.H * 0.5 + 0.06;
    const targetY = Math.max(feetY + d.rideHeight * d.legLength + this.H * 0.3, floor);
    // Critically-ish damped spring: footfalls kick it, so the body bobs.
    this.bodyVy += ((targetY - this.bodyY) * 70 - this.bodyVy * 13) * dt;
    this.bodyY += this.bodyVy * dt;
    if (this.bodyY < floor) {
      this.bodyY = floor;
      this.bodyVy = Math.max(0, this.bodyVy);
    }

    // Tilt to the plane of the feet, softened towards world up.
    const avg = (pred: (l: Leg) => boolean) => {
      const ls = this.legs.filter(pred);
      return ls.reduce<V3>((a, l) => add(a, scale(l.foot, 1 / ls.length)), [0, 0, 0]);
    };
    const fwdH: V3 = [Math.sin(this.yaw), 0, Math.cos(this.yaw)];
    const sideH: V3 = [Math.cos(this.yaw), 0, -Math.sin(this.yaw)];
    const fv = d.legPairs > 1 ? norm(sub(avg((l) => l.pair === 0), avg((l) => l.pair === d.legPairs - 1)), fwdH) : fwdH;
    const sv = norm(sub(avg((l) => l.side > 0), avg((l) => l.side < 0)), sideH);
    let upT = norm(cross(fv, sv));
    if (upT[1] < 0) upT = scale(upT, -1);
    upT = norm(lerp(upT, UP, 0.45));
    // Lean a little into turns.
    upT = norm(addScaled(upT, sideH, -this.yawRate * len(this.vel) * 0.04));
    this.up = norm(lerp(this.up, upT, damp(6, dt)));
    const fwd = norm(sub(fwdH, scale(this.up, dot(fwdH, this.up))));
    this.frame = { p: [this.pos[0], this.bodyY, this.pos[2]], q: qLook(fwd, this.up) };
  }

  private canStep(leg: Leg, err: number) {
    const stepping = this.legs.filter((l) => l.stepping);
    const urgent = err > this.stride * 1.7;
    switch (this.design.gait) {
      case 'creep':
        return stepping.length === 0 || (urgent && stepping.length < 2);
      case 'scurry':
        return stepping.length < Math.max(1, this.legs.length / 2);
      case 'trot': {
        // Neighbours (same side, adjacent pair; or the opposite leg) stay planted.
        const busy = stepping.some(
          (l) => (l.side === leg.side && Math.abs(l.pair - leg.pair) === 1) || (l.side !== leg.side && l.pair === leg.pair),
        );
        return !busy || (urgent && stepping.length < this.legs.length - 1);
      }
    }
  }

  private updateLegs(dt: number) {
    const dur = this.stepDuration();
    const speed = len(this.vel) + Math.abs(this.yawRate) * 0.5;
    for (const l of this.legs) l.desired = this.restWorld(l.rest, dur * 0.7);

    const candidates = this.legs
      .filter((l) => !l.stepping)
      .map((l) => ({ l, err: distXZ(l.foot, l.desired) + Math.abs(l.foot[1] - l.desired[1]) * 0.5 }))
      .filter(({ err }) => err > this.stride || (speed < 0.08 && err > 0.07))
      .sort((a, b) => b.err - a.err);
    for (const { l, err } of candidates) {
      if (!this.canStep(l, err)) continue;
      l.stepping = true;
      l.t = 0;
      l.from = copy(l.foot);
    }

    for (const l of this.legs) {
      if (!l.stepping) continue;
      l.t = Math.min(1, l.t + dt / dur);
      const e = smoothstep(l.t);
      const span = distXZ(l.from, l.desired);
      const lift = this.design.stepHeight * (0.5 + 0.5 * Math.min(1, span / this.stride)) + Math.max(0, l.desired[1] - l.from[1]) * 0.6;
      const p = lerp(l.from, l.desired, e);
      p[1] = Math.max(mix(l.from[1], l.desired[1], e) + Math.sin(Math.PI * l.t) * lift, this.world.heightAt(p[0], p[2]) + 0.03);
      l.foot = p;
      if (l.t >= 1) {
        l.stepping = false;
        l.foot = copy(l.desired);
        this.bodyVy -= 0.25 * Math.min(1, span / this.stride) * this.design.stepHeight * 4 / this.legs.length;
        this.puff(l.foot, span);
      }
    }
  }

  private solveLegs() {
    const up = qRotate(this.frame.q, UP);
    const fwd = qRotate(this.frame.q, [0, 0, 1]);
    for (const l of this.legs) {
      const hip = toWorld(this.frame, l.hip);
      const out = qRotate(this.frame.q, [l.side, 0, 0]);
      const total = this.design.legLength;
      const mid = lerp(hip, l.foot, 0.5);
      // Bipeds bend their knees forward; many-legged things arch up and out like spiders.
      const pole = this.legs.length === 2
        ? add(mid, add(scale(fwd, total * 0.6), scale(up, total * 0.15)))
        : add(mid, add(scale(up, total * 0.65), scale(out, total * 0.2)));
      const chain = bentChain(hip, pole, l.foot, l.lengths.length + 1);
      fabrik(chain, l.lengths, l.foot);
      l.joints = chain;
    }
  }

  private updateHead(dt: number, time: number) {
    const target = toWorld(this.frame, this.headAnchor());
    const acc = sub(scale(sub(target, this.headPos), 110), scale(this.headVel, 13));
    this.headVel = addScaled(this.headVel, acc, dt);
    this.headPos = addScaled(this.headPos, this.headVel, dt);
    if (dist(this.headPos, target) > 0.35) this.headPos = add(target, scale(norm(sub(this.headPos, target)), 0.35));

    const fwd = qRotate(this.frame.q, [0, 0, 1]);
    const look = this.lookPoint ?? this.goal ?? addScaled(this.headPos, fwd, 3);
    let want = norm(sub(look, this.headPos), fwd);
    want = norm([want[0], clamp(want[1], -0.5, 0.4), want[2]]);
    if (dot(want, fwd) < 0.3) want = norm(lerp(fwd, want, 0.4));
    this.headDir = norm(lerp(this.headDir, want, damp(5, dt)));
    this.blink = (time % 3.7) < 0.12 ? 0.15 : 1;
  }

  private headFrame(): Frame {
    return { p: this.headPos, q: qLook(this.headDir, this.up) };
  }

  private updateTail(dt: number, time: number) {
    if (this.tail.length < 2) return;
    const seg = 0.14;
    const back = qRotate(this.frame.q, [0, 0, -1]);
    const side = qRotate(this.frame.q, [1, 0, 0]);
    this.tail[0] = toWorld(this.frame, [0, this.H * 0.1, -this.L / 2]);
    const wag = 0.15 + len(this.vel) * 0.4;
    for (let k = 1; k < this.tail.length; k++) {
      const prev = this.tail[k - 1];
      const curl: V3 = norm(add(back, [0, 0.45 - k * 0.12, 0]));
      const rest = addScaled(addScaled(prev, curl, seg), side, Math.sin(time * 5 - k * 0.7) * wag * 0.04 * k);
      let p = lerp(this.tail[k], rest, damp(5, dt));
      p = addScaled(prev, norm(sub(p, prev), back), seg);
      p[1] = Math.max(p[1], this.world.heightAt(p[0], p[2]) + 0.05);
      this.tail[k] = p;
    }
  }

  private updateAntennae(dt: number) {
    const hf = this.headFrame();
    const back = qRotate(hf.q, [0, 0, -1]);
    for (const a of this.antennae) {
      const base = toWorld(hf, a.base);
      const sideBias = qRotate(hf.q, [a.base[0] * 2, 0, 0]);
      const restDir = norm(add(add(this.up, scale(back, 0.35)), sideBias));
      a.pts[0] = base;
      a.prev[0] = base;
      for (let k = 1; k < a.pts.length; k++) {
        const p = a.pts[k];
        const v = scale(sub(p, a.prev[k]), 0.9);
        a.prev[k] = copy(p);
        let n = add(add(p, v), [0, -1.5 * dt * dt, 0]);
        n = lerp(n, addScaled(base, restDir, k * 0.11), damp(9, dt));
        a.pts[k] = n;
      }
      for (let it = 0; it < 3; it++) {
        for (let k = 1; k < a.pts.length; k++) {
          a.pts[k] = addScaled(a.pts[k - 1], norm(sub(a.pts[k], a.pts[k - 1]), restDir), 0.11);
        }
      }
    }
  }

  private puff(at: V3, strength: number) {
    const r = Math.random;
    const n = 2 + Math.round(Math.min(1, strength / this.stride) * 3);
    for (let i = 0; i < n; i++) {
      const a = r() * Math.PI * 2;
      this.dust.push({
        p: add(at, [0, 0.03, 0]),
        v: [Math.cos(a) * (0.4 + r() * 0.4), 0.35 + r() * 0.4, Math.sin(a) * (0.4 + r() * 0.4)],
        life: 0, max: 0.35 + r() * 0.35, size: 0.06 + r() * 0.06,
      });
    }
  }

  private updateDust(dt: number) {
    for (const d of this.dust) {
      d.life += dt;
      d.p = addScaled(d.p, d.v, dt);
      d.v = scale(d.v, 1 - damp(4, dt));
    }
    this.dust = this.dust.filter((d) => d.life < d.max);
  }

  draw(list: DrawList) {
    const P = this.pal;
    for (const part of this.parts) list.addLocal(this.frame, part.prim, part.p, part.q, part.s, part.col, part.mat);
    this.drawLegs(list);
    this.drawHead(list);

    for (let k = 1; k < this.tail.length; k++) {
      const a = this.tail[k - 1], b = this.tail[k];
      const r = mix(0.13, 0.05, k / this.tail.length) * (0.6 + this.W * 0.5);
      seg(list, Prim.Cyl, a, b, [r, 1.1, r], k % 2 ? P.second : P.dark, qRotate(this.frame.q, [1, 0, 0]));
      list.add(Prim.Sphere, b, qIdentity(), [r * 1.15, r * 1.15, r * 1.15], k % 2 ? P.dark : P.main);
    }
    if (this.tail.length > 1) {
      const tip = this.tail[this.tail.length - 1];
      list.add(Prim.Sphere, tip, qIdentity(), [0.09, 0.09, 0.09], P.glow, Mat.Emissive);
    }

    for (const a of this.antennae) {
      for (let k = 1; k < a.pts.length; k++) seg(list, Prim.Box, a.pts[k - 1], a.pts[k], [0.025, 1, 0.025], P.dark, [1, 0, 0]);
      list.add(Prim.Sphere, a.pts[a.pts.length - 1], qIdentity(), [0.07, 0.07, 0.07], P.glow, Mat.Emissive);
    }

    for (const d of this.dust) {
      const s = d.size * (1 - d.life / d.max);
      list.add(Prim.Box, d.p, qIdentity(), [s, s, s], [0.78, 0.7, 0.56]);
    }
  }

  private drawLegs(list: DrawList) {
    const P = this.pal, th = this.th;
    const mountQ = qAxisAngle([0, 0, 1], Math.PI / 2);
    for (const l of this.legs) {
      list.addLocal(this.frame, Prim.Cyl, l.hip, mountQ, [th * 2.4, th * 1.6, th * 2.4], P.dark);
      const out = qRotate(this.frame.q, [l.side, 0, 0]);
      for (let i = 1; i < l.joints.length; i++) {
        const a = l.joints[i - 1], b = l.joints[i];
        const taper = 1 - (i - 1) / l.joints.length * 0.35;
        const t = th * taper;
        if (this.design.legStyle === 'pipe') {
          seg(list, Prim.Cyl, a, b, [t, 1, t], P.second, out);
        } else if (this.design.legStyle === 'girder') {
          seg(list, Prim.Box, a, b, [t * 1.1, 1, t * 1.7], P.main, out);
          seg(list, Prim.Box, a, b, [t * 0.45, 0.85, t * 1.9], P.dark, out);
        } else {
          seg(list, Prim.Cyl, a, lerp(a, b, 0.6), [t * 1.6, 1, t * 1.6], P.main, out);
          seg(list, Prim.Cyl, a, b, [t * 0.8, 1, t * 0.8], P.second, out);
        }
        if (i < l.joints.length - 1) list.add(Prim.Sphere, b, qIdentity(), [t * 1.7, t * 1.7, t * 1.7], P.dark);
      }
      const foot = l.joints[l.joints.length - 1];
      list.add(Prim.Cone, add(foot, [0, th * 1.3, 0]), qAxisAngle([1, 0, 0], Math.PI), [th * 2.4, th * 2.6, th * 2.4], P.dark);
      list.add(Prim.Cyl, add(foot, [0, th * 2.5, 0]), qIdentity(), [th * 2.2, th * 0.6, th * 2.2], P.accent);
    }
  }

  private drawHead(list: DrawList) {
    const P = this.pal, hs = this.hs;
    const hf = this.headFrame();
    // Springy neck: a few coils between the chassis and the bobbing head.
    const anchor = toWorld(this.frame, [0, this.H * 0.4, this.L / 2 - 0.05]);
    for (let i = 0; i < 4; i++) {
      seg(list, Prim.Cyl, lerp(anchor, hf.p, i / 4), lerp(anchor, hf.p, (i + 1) / 4), [0.12 + (i % 2) * 0.04, 0.7, 0.12 + (i % 2) * 0.04], i % 2 ? P.dark : P.second, [1, 0, 0]);
    }
    const lens = (z: number, d: number) => {
      list.addLocal(hf, Prim.Cyl, [0, 0, z - 0.02], X_TO_Z, [d * 1.3, 0.05, d * 1.3], P.dark);
      list.addLocal(hf, Prim.Cyl, [0, 0, z], X_TO_Z, [d, 0.05, d * this.blink], P.glow, Mat.Emissive);
    };
    if (this.design.head === 'can') {
      list.addLocal(hf, Prim.Cyl, [0, 0, 0], qIdentity(), [hs, hs * 1.15, hs], P.second);
      list.addLocal(hf, Prim.Cyl, [0, hs * 0.58, 0], qIdentity(), [hs * 1.06, 0.05, hs * 1.06], P.dark);
      lens(hs * 0.5, hs * 0.45);
    } else if (this.design.head === 'tv') {
      list.addLocal(hf, Prim.Box, [0, 0, 0], qIdentity(), [hs * 1.45, hs, hs], P.main);
      list.addLocal(hf, Prim.Box, [0, 0, hs * 0.5], qIdentity(), [hs * 1.15, hs * 0.72, 0.03], P.dark);
      for (const x of [-1, 1]) {
        list.addLocal(hf, Prim.Box, [x * hs * 0.27, 0, hs * 0.52], qIdentity(), [hs * 0.2, hs * 0.28 * this.blink, 0.03], P.glow, Mat.Emissive);
      }
      list.addLocal(hf, Prim.Cyl, [hs * 0.55, hs * 0.2, hs * 0.5], X_TO_Z, [0.06, 0.04, 0.06], P.accent);
    } else {
      list.addLocal(hf, Prim.Cone, [0, 0, 0], qAxisAngle([1, 0, 0], -Math.PI / 2), [hs * 1.25, hs * 1.1, hs * 1.25], P.main);
      list.addLocal(hf, Prim.Cyl, [0, 0, -hs * 0.55], X_TO_Z, [hs * 0.35, hs * 0.3, hs * 0.35], P.dark);
      lens(hs * 0.56, hs * 1.05);
    }
  }
}

/** A primitive stretched between two points (unit length along local Y). */
function seg(list: DrawList, prim: Prim, a: V3, b: V3, s: V3, col: V3, side: V3, mat?: Mat) {
  const d = sub(b, a);
  const l = len(d);
  if (l < 1e-4) return;
  list.add(prim, lerp(a, b, 0.5), qAlongY(d, side), [s[0], s[1] * l, s[2]], col, mat);
}

/** Bolts random scrap onto a hull. The seed decides the junk; the sliders decide the size. */
function buildChassis(d: Design, r: Rng, P: Palette, L: number, W: number, H: number): Part[] {
  const parts: Part[] = [];
  const add_ = (prim: Prim, p: V3, s: V3, col: V3, q: Quat = qIdentity(), mat?: Mat) => parts.push({ prim, p, q, s, col, mat });
  if (d.hull === 'crate') {
    add_(Prim.Box, [0, 0, 0], [W, H, L], P.main);
    add_(Prim.Box, [0, -H * 0.42, 0], [W + 0.04, H * 0.16, L + 0.04], P.dark);
  } else if (d.hull === 'drum') {
    add_(Prim.Cyl, [0, 0, 0], [W, L, H * 1.25], P.main, X_TO_Z);
    for (const z of [-L * 0.32, L * 0.32]) add_(Prim.Cyl, [0, 0, z], [W + 0.04, 0.06, H * 1.25 + 0.04], P.dark, X_TO_Z);
  } else {
    add_(Prim.Sphere, [0, 0, 0], [W * 1.05, H * 1.3, L * 1.05], P.main);
    add_(Prim.Cyl, [0, -H * 0.1, 0], [W * 1.06, 0.08, L * 1.06], P.dark);
  }
  const top = d.hull === 'drum' ? H * 0.6 : d.hull === 'pod' ? H * 0.62 : H / 2;

  // Junk on top. Each piece is optional so every seed looks a little different.
  if (r.chance(0.7)) {
    const q = qAxisAngle(UP, r.range(-0.25, 0.25));
    add_(Prim.Box, [r.range(-0.05, 0.05) * W, top + 0.02, r.range(-0.15, 0.1) * L], [W * r.range(0.45, 0.75), 0.05, L * r.range(0.35, 0.6)], P.second, q);
  }
  if (r.chance(0.6)) {
    const z = -L * r.range(0.25, 0.4), x = r.range(-0.3, 0.3) * W;
    add_(Prim.Cyl, [x, top + 0.17, z], [0.1, 0.36, 0.1], P.dark);
    add_(Prim.Cyl, [x, top + 0.37, z], [0.14, 0.05, 0.14], P.accent);
  }
  if (r.chance(0.55)) {
    const x = r.range(-0.2, 0.2) * W, z = r.range(-0.1, 0.25) * L;
    add_(Prim.Box, [x, top + 0.08, z], [0.26, 0.16, 0.18], P.accent);
    for (const dx of [-0.07, 0.07]) add_(Prim.Cyl, [x + dx, top + 0.18, z], [0.04, 0.05, 0.04], P.dark);
  }
  if (r.chance(0.5)) {
    const s = r.chance(0.5) ? 1 : -1;
    add_(Prim.Cyl, [s * W * 0.3, top + 0.05, 0], [0.07, L * 0.8, 0.07], P.second, X_TO_Z);
  }
  if (r.chance(0.5)) add_(Prim.Wedge, [0, -H * 0.25, L / 2 + 0.14], [W * 0.85, H * 0.45, 0.28], P.second);
  for (const s of [-1, 1]) {
    if (r.chance(0.6)) {
      const z = r.range(-0.25, 0.25) * L;
      add_(Prim.Box, [s * (W / 2 + 0.015), r.range(-0.1, 0.15) * H, z], [0.03, H * r.range(0.4, 0.7), L * r.range(0.25, 0.45)], r.chance(0.5) ? P.second : P.accent);
    }
  }
  const rivets = r.int(2, 4);
  for (let i = 0; i < rivets; i++) {
    for (const s of [-1, 1]) add_(Prim.Sphere, [s * W * 0.42, top, mix(-L * 0.42, L * 0.42, (i + 0.5) / rivets)], [0.06, 0.06, 0.06], P.dark);
  }
  if (r.chance(0.6)) add_(Prim.Sphere, [r.range(-0.3, 0.3) * W, top + 0.04, -L * 0.45], [0.09, 0.09, 0.09], P.glow, qIdentity(), Mat.Emissive);
  return parts;
}
