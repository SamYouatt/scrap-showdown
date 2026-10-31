// Hull mount points and the tools that bolt onto them. Like the rest of the
// creature, tools are animated procedurally: the wedge plow hinges so its lip
// rides the ground, and the pincer runs off a pneumatics tank through a hose.
import type { Palette } from './creature';
import { DrawList, Mat } from './render/drawlist';
import { Prim } from './render/mesh';
import { Frame, V3, add, addScaled, clamp, damp, lerp, mix, norm, qAxisAngle, qIdentity, qMul, qRotate, scale, toWorld } from './math';
import type { Terrain } from './world';

/** Every mount on a hull. Leg kits use both side mounts at once. */
export type SlotId = 'legs' | 'head' | 'front' | 'spine' | 'back';
export type ToolSlot = 'front' | 'spine' | 'back';
export type ToolId = 'wedge' | 'pincer' | 'pneumatics';
export type Tools = Partial<Record<ToolSlot, ToolId>>;

export interface Dims {
  L: number;
  W: number;
  H: number;
}

export interface Mount {
  slot: SlotId;
  /** Hull-local position on the surface, and the surface normal. */
  p: V3;
  n: V3;
  /** Hull-local point to grab whatever is fitted here. */
  grab: V3;
}

/** Mount points on the box hull. */
export function mountPoints({ L, W, H }: Dims): Mount[] {
  const top = H / 2;
  return [
    { slot: 'legs', p: [W / 2, -H * 0.05, 0], n: [1, 0, 0], grab: [W / 2 + 0.15, -H * 0.05, 0] },
    { slot: 'legs', p: [-W / 2, -H * 0.05, 0], n: [-1, 0, 0], grab: [-W / 2 - 0.15, -H * 0.05, 0] },
    { slot: 'head', p: [0, top, L / 2 - 0.14], n: [0, 1, 0], grab: [0, top + 0.35, L / 2 + 0.1] },
    { slot: 'spine', p: [0, top, 0], n: [0, 1, 0], grab: [0, top + 0.17, 0] },
    { slot: 'back', p: [0, top, -L / 2 + 0.2], n: [0, 1, 0], grab: [0, top + 0.17, -L / 2 + 0.2] },
    { slot: 'front', p: [0, -H * 0.05, L / 2], n: [0, 0, 1], grab: [0, -H * 0.1, L / 2 + 0.3] },
  ];
}

const X_TO_Z = qAxisAngle([1, 0, 0], Math.PI / 2);
const Z_TO_X = qAxisAngle([0, 0, 1], Math.PI / 2);
const TANK_R = 0.15;
const TANK_LEN = 0.46;
const PLOW_LEN = 0.5;
const CLAW_PERIOD = 3.4;

interface Puff {
  p: V3;
  v: V3;
  life: number;
  max: number;
}

export class Toolkit {
  private plowAngle = 0.6;
  private plowVel = 0;
  private clawOpen = 0.8;
  private clawExt = 0;
  private clawDroop = 0.5;
  private clawPhase = 0;
  private puffs: Puff[] = [];
  private readonly front: V3;
  private readonly tankAt: V3 | null;

  constructor(private tools: Tools, private dims: Dims, private ground: Terrain) {
    const m = mountPoints(dims);
    this.front = m.find((x) => x.slot === 'front')!.p;
    const tankSlot = (['spine', 'back'] as const).find((s) => tools[s] === 'pneumatics');
    this.tankAt = tankSlot ? m.find((x) => x.slot === tankSlot)!.p : null;
  }

  /** The pincer only works with a pneumatics tank somewhere on the hull. */
  get powered() {
    return this.tools.front === 'pincer' && this.tankAt !== null;
  }

  update(dt: number, time: number, frame: Frame) {
    if (this.tools.front === 'wedge') this.updatePlow(dt, frame);
    if (this.tools.front === 'pincer') this.updateClaw(dt, time, frame);
    for (const p of this.puffs) {
      p.life += dt;
      p.p = addScaled(p.p, p.v, dt);
      p.v = scale(p.v, 1 - damp(5, dt));
    }
    this.puffs = this.puffs.filter((p) => p.life < p.max);
  }

  draw(list: DrawList, frame: Frame, P: Palette) {
    if (this.tools.front === 'wedge') this.drawPlow(list, frame, P);
    if (this.tools.front === 'pincer') this.drawClaw(list, frame, P);
    for (const s of ['spine', 'back'] as const) {
      if (this.tools[s] === 'pneumatics') this.drawTank(list, frame, P, mountPoints(this.dims).find((m) => m.slot === s)!.p);
    }
    if (this.powered) this.drawHose(list, frame, P);
    for (const p of this.puffs) {
      const s = 0.07 + (p.life / p.max) * 0.12;
      list.add(Prim.Box, p.p, qIdentity(), [s, s, s], [0.93, 0.95, 0.96]);
    }
  }

  // ---------------------------------------------------------------- wedge plow

  private plowHinge(): V3 {
    return [0, -this.dims.H * 0.3, this.dims.L / 2 + 0.03];
  }

  /** The plate swings on its hinge so the lip scrapes along the ground. */
  private updatePlow(dt: number, frame: Frame) {
    const hinge = toWorld(frame, this.plowHinge());
    const fwd = qRotate(frame.q, [0, 0, 1]);
    const at = addScaled(hinge, fwd, PLOW_LEN * 0.85);
    const drop = hinge[1] - this.ground.heightAt(at[0], at[2]) - 0.03;
    const target = Math.asin(clamp(drop / PLOW_LEN, Math.sin(0.12), Math.sin(1.25)));
    this.plowVel += ((target - this.plowAngle) * 90 - this.plowVel * 12) * dt;
    this.plowAngle = Math.min(this.plowAngle + this.plowVel * dt, target);
  }

  private drawPlow(list: DrawList, frame: Frame, P: Palette) {
    const { W, H, L } = this.dims;
    const width = W * 1.2;
    const pf: Frame = { p: toWorld(frame, this.plowHinge()), q: qMul(frame.q, qAxisAngle([1, 0, 0], this.plowAngle)) };
    list.addLocal(pf, Prim.Box, [0, 0, PLOW_LEN / 2], qIdentity(), [width, 0.04, PLOW_LEN], P.main);
    // Hazard band across the plate.
    const n = 6;
    for (let i = 0; i < n; i++) {
      const x = (i + 0.5 - n / 2) * (width / n);
      list.addLocal(pf, Prim.Box, [x, 0.022, PLOW_LEN * 0.55], qIdentity(), [width / n, 0.01, PLOW_LEN * 0.3], i % 2 ? P.dark : P.accent);
    }
    list.addLocal(pf, Prim.Box, [0, -0.01, PLOW_LEN], qIdentity(), [width + 0.02, 0.06, 0.06], P.dark);
    for (const s of [-1, 1]) {
      list.addLocal(pf, Prim.Wedge, [s * width / 2, 0.08, PLOW_LEN * 0.45], qIdentity(), [0.03, 0.16, PLOW_LEN * 0.9], P.second);
      // Struts from the hull to the plate.
      const a = toWorld(frame, [s * W * 0.3, H * 0.2, L / 2]);
      const b = toWorld(pf, [s * W * 0.3, 0.02, PLOW_LEN * 0.55]);
      list.seg(Prim.Cyl, a, b, [0.05, 1, 0.05], P.second, qRotate(frame.q, [1, 0, 0]));
    }
    list.addLocal(pf, Prim.Cyl, [0, 0, 0], Z_TO_X, [0.08, width * 0.9, 0.08], P.dark);
  }

  // ---------------------------------------------------------------- pincer

  /** Powered: snap shut, hold, then open and retract. Unpowered: the jaws hang limp. */
  private updateClaw(dt: number, time: number, frame: Frame) {
    const phase = (time % CLAW_PERIOD) / CLAW_PERIOD;
    let open = 0.85, ext = 0, droop = 0.5 + Math.sin(time * 1.3) * 0.05, rate = 3;
    if (this.powered) {
      droop = Math.sin(time * 2) * 0.03;
      if (phase < 0.3) {
        open = 0.05;
        ext = 1;
        rate = 30;
      } else {
        open = 1;
        rate = 4;
      }
      if (phase < this.clawPhase) this.hiss(frame);
    }
    this.clawPhase = phase;
    this.clawOpen = mix(this.clawOpen, open, damp(rate, dt));
    this.clawExt = mix(this.clawExt, ext, damp(rate, dt));
    this.clawDroop = mix(this.clawDroop, droop, damp(4, dt));
  }

  private hiss(frame: Frame) {
    const at = [toWorld(frame, add(this.front, [0, 0, 0.3]))];
    if (this.tankAt) at.push(toWorld(frame, this.tankOutlet(this.tankAt)));
    for (const p of at) {
      for (let i = 0; i < 3; i++) {
        const a = Math.random() * Math.PI * 2;
        this.puffs.push({ p, v: [Math.cos(a) * 0.5, 0.4 + Math.random() * 0.4, Math.sin(a) * 0.5], life: 0, max: 0.3 + Math.random() * 0.2 });
      }
    }
  }

  private drawClaw(list: DrawList, frame: Frame, P: Palette) {
    const F = this.front;
    const right = qRotate(frame.q, [1, 0, 0]);
    const W = (v: V3) => toWorld(frame, v);
    list.addLocal(frame, Prim.Box, add(F, [0, 0, 0.03]), qIdentity(), [0.28, 0.22, 0.07], P.dark);
    const d: V3 = [0, -Math.sin(this.clawDroop), Math.cos(this.clawDroop)];
    const reach = 0.24 + this.clawExt * 0.18;
    list.seg(Prim.Cyl, W(F), W(addScaled(F, d, 0.22)), [0.11, 1, 0.11], P.main, right);
    list.seg(Prim.Cyl, W(addScaled(F, d, 0.15)), W(addScaled(F, d, reach)), [0.06, 1, 0.06], P.second, right);
    const C = addScaled(F, d, reach);
    list.add(Prim.Box, W(C), qMul(frame.q, qAxisAngle([1, 0, 0], this.clawDroop)), [0.22, 0.12, 0.08], P.accent);
    const a = mix(0.08, 0.65, this.clawOpen);
    for (const s of [-1, 1]) {
      const base = addScaled(C, [1, 0, 0], s * 0.07);
      const j1 = norm(addScaled(d, [1, 0, 0], s * Math.tan(a)));
      const knee = addScaled(base, j1, 0.2);
      const tip = addScaled(knee, norm(addScaled(d, [1, 0, 0], -s * 0.45)), 0.16);
      list.seg(Prim.Box, W(base), W(knee), [0.06, 1, 0.08], P.second, right);
      list.seg(Prim.Box, W(knee), W(tip), [0.05, 1, 0.07], P.dark, right);
      list.add(Prim.Sphere, W(knee), qIdentity(), [0.07, 0.07, 0.07], P.dark);
    }
    // Air indicator: green when the tank is plumbed in, red when it isn't.
    const lit = this.powered || Math.floor(performance.now() / 400) % 2 === 0;
    const col: V3 = this.powered ? [0.45, 1, 0.4] : lit ? [1, 0.25, 0.2] : [0.35, 0.12, 0.1];
    list.addLocal(frame, Prim.Sphere, add(F, [0.09, 0.12, 0.04]), qIdentity(), [0.06, 0.06, 0.06], col, Mat.Emissive);
  }

  // ---------------------------------------------------------------- pneumatics

  private tankCentre(at: V3): V3 {
    return add(at, [0, TANK_R + 0.04, 0]);
  }

  private tankOutlet(at: V3): V3 {
    return add(this.tankCentre(at), [0.07, 0.06, TANK_LEN / 2 + 0.06]);
  }

  private drawTank(list: DrawList, frame: Frame, P: Palette, at: V3) {
    const c = this.tankCentre(at);
    const d = TANK_R * 2;
    list.addLocal(frame, Prim.Cyl, c, X_TO_Z, [d, TANK_LEN, d], P.accent);
    for (const z of [-1, 1]) list.addLocal(frame, Prim.Sphere, add(c, [0, 0, z * TANK_LEN / 2]), qIdentity(), [d, d, TANK_R * 0.9], P.accent);
    for (const z of [-0.12, 0.12]) {
      list.addLocal(frame, Prim.Cyl, add(c, [0, 0, z]), X_TO_Z, [d + 0.03, 0.04, d + 0.03], P.dark);
      list.addLocal(frame, Prim.Box, add(at, [0, 0.03, z]), qIdentity(), [0.26, 0.06, 0.05], P.dark);
    }
    // Pressure gauge on top: glows while it's feeding the pincer.
    list.addLocal(frame, Prim.Cyl, add(c, [-0.04, TANK_R + 0.02, 0.05]), qIdentity(), [0.12, 0.04, 0.12], P.second);
    list.addLocal(frame, Prim.Cyl, add(c, [-0.04, TANK_R + 0.04, 0.05]), qIdentity(), [0.07, 0.02, 0.07], this.powered ? P.glow : P.dark, this.powered ? Mat.Emissive : Mat.Lit);
    const o = this.tankOutlet(at);
    list.addLocal(frame, Prim.Cyl, add(o, [0, 0, -0.04]), X_TO_Z, [0.06, 0.1, 0.06], P.dark);
  }

  /** A rubber hose from the tank, over the nose of the hull, down to the pincer. */
  private drawHose(list: DrawList, frame: Frame, P: Palette) {
    const { W, H, L } = this.dims;
    const p0 = this.tankOutlet(this.tankAt!);
    const p3 = add(this.front, [0.1, 0.11, 0.05]);
    const p1: V3 = [W * 0.32, H / 2 + 0.2, p0[2] + 0.25];
    const p2: V3 = [W * 0.34, H / 2 + 0.18, L / 2 + 0.1];
    const bez = (t: number): V3 => {
      const a = lerp(p0, p1, t), b = lerp(p1, p2, t), c = lerp(p2, p3, t);
      return lerp(lerp(a, b, t), lerp(b, c, t), t);
    };
    const right = qRotate(frame.q, [1, 0, 0]);
    const n = 12;
    for (let i = 0; i < n; i++) {
      list.seg(Prim.Cyl, toWorld(frame, bez(i / n)), toWorld(frame, bez((i + 1) / n)), [0.07, 1.15, 0.07], P.dark, right);
    }
  }
}
