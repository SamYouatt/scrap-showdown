// Hull mount points and the tools that bolt onto them. Each mount has a facing
// (`aim`), and whatever is fitted there points that way: a pincer on the back
// top reaches backwards. Like the rest of the creature, tools are animated
// procedurally: the wedge plow hinges so its lip rides the ground, and pincers
// run off a pneumatics tank through a hose.
import type { Palette } from './creature';
import { DrawList, Mat } from './render/drawlist';
import { Prim } from './render/mesh';
import { Frame, Quat, V3, add, addScaled, clamp, damp, lerp, mix, norm, qAxisAngle, qIdentity, qLook, qMul, qRotate, scale, toWorld } from './math';
import type { Terrain } from './world';

/** Every mount on a hull. Leg kits use both side mounts at once. */
export type SlotId = 'legs' | 'fronttop' | 'front' | 'spine' | 'back';
/** Mounts that take a single part: tools or the control module. */
export type ToolSlot = Exclude<SlotId, 'legs'>;
export type ToolId = 'wedge' | 'pincer' | 'pneumatics';
export type Tools = Partial<Record<ToolSlot, ToolId>>;

export interface Dims {
  L: number;
  W: number;
  H: number;
}

export interface Mount {
  slot: SlotId;
  /** Hull-local position on the surface, the surface normal, and the way fitted parts face. */
  p: V3;
  n: V3;
  aim: V3;
  /** Hull-local point to grab whatever is fitted here. */
  grab: V3;
}

/** Mount points on the box hull. */
export function mountPoints({ L, W, H }: Dims): Mount[] {
  const top = H / 2;
  const up: V3 = [0, 1, 0], fwd: V3 = [0, 0, 1], back: V3 = [0, 0, -1];
  return [
    { slot: 'legs', p: [W / 2, -H * 0.05, 0], n: [1, 0, 0], aim: [1, 0, 0], grab: [W / 2 + 0.15, -H * 0.05, 0] },
    { slot: 'legs', p: [-W / 2, -H * 0.05, 0], n: [-1, 0, 0], aim: [-1, 0, 0], grab: [-W / 2 - 0.15, -H * 0.05, 0] },
    { slot: 'fronttop', p: [0, top, L / 2 - 0.16], n: up, aim: fwd, grab: [0, top + 0.3, L / 2 + 0.05] },
    { slot: 'spine', p: [0, top, 0], n: up, aim: fwd, grab: [0, top + 0.2, 0] },
    { slot: 'back', p: [0, top, -L / 2 + 0.2], n: up, aim: back, grab: [0, top + 0.2, -L / 2 + 0.1] },
    { slot: 'front', p: [0, -H * 0.05, L / 2], n: fwd, aim: fwd, grab: [0, -H * 0.1, L / 2 + 0.3] },
  ];
}

export const mountAt = (dims: Dims, slot: ToolSlot) => mountPoints(dims).find((m) => m.slot === slot)!;

/** Hull-local frame of a mount: +Z along its aim, +Y up. */
function mountQ(m: Mount): Quat {
  return qLook(m.aim, [0, 1, 0]);
}

const compose = (f: Frame, p: V3, q: Quat): Frame => ({ p: toWorld(f, p), q: qMul(f.q, q) });

const X_TO_Z = qAxisAngle([1, 0, 0], Math.PI / 2);
const Z_TO_X = qAxisAngle([0, 0, 1], Math.PI / 2);
const TANK_R = 0.15;
const TANK_LEN = 0.46;
const PLOW_LEN = 0.5;
const CLAW_PERIOD = 3.4;
/** How far past its mount an extended pincer closes. */
export const CLAW_REACH = 0.75;

interface Puff {
  p: V3;
  v: V3;
  life: number;
  max: number;
}

interface Claw {
  m: Mount;
  /** Pincers on top of the hull stand on a short pedestal; ones on a face bolt straight on. */
  base: V3;
  open: number;
  ext: number;
  droop: number;
  phase: number;
  snapAt: number;
  grip: boolean;
  hiss: boolean;
}

export class Toolkit {
  /** In a fight, pincers only snap when told to (see snap and grip) instead of idling. */
  manual = false;
  private plowAngle = 0.6;
  private plowVel = 0;
  private claws: Claw[] = [];
  private tanks: Mount[] = [];
  private puffs: Puff[] = [];
  private time = 0;

  constructor(private tools: Tools, private dims: Dims, private ground: Terrain) {
    for (const [slot, id] of Object.entries(tools) as [ToolSlot, ToolId][]) {
      const m = mountAt(dims, slot);
      if (id === 'pneumatics') this.tanks.push(m);
      if (id === 'pincer') {
        const base: V3 = slot === 'front' ? [0, 0, 0] : [0, 0.12, 0];
        this.claws.push({ m, base, open: 0.8, ext: 0, droop: 0.5, phase: 0, snapAt: -1e9, grip: false, hiss: false });
      }
    }
  }

  /** Pincers only work with a pneumatics tank somewhere on the hull. */
  get powered() {
    return this.tanks.length > 0;
  }

  get pincers(): ToolSlot[] {
    return this.claws.map((c) => c.m.slot as ToolSlot);
  }

  snap(slot: ToolSlot) {
    const c = this.claws.find((x) => x.m.slot === slot);
    if (!c || !this.powered) return;
    c.snapAt = this.time;
    c.hiss = true;
  }

  grip(slot: ToolSlot, on: boolean) {
    const c = this.claws.find((x) => x.m.slot === slot);
    if (c) c.grip = on && this.powered;
  }

  /** World position where a pincer's jaws close when fully extended. */
  clawTip(frame: Frame, slot: ToolSlot): V3 {
    const m = mountAt(this.dims, slot);
    return toWorld(frame, addScaled(m.p, m.aim, CLAW_REACH));
  }

  update(dt: number, time: number, frame: Frame) {
    this.time = time;
    if (this.tools.front === 'wedge') this.updatePlow(dt, frame);
    for (const c of this.claws) this.updateClaw(c, dt, time, frame);
    for (const p of this.puffs) {
      p.life += dt;
      p.p = addScaled(p.p, p.v, dt);
      p.v = scale(p.v, 1 - damp(5, dt));
    }
    this.puffs = this.puffs.filter((p) => p.life < p.max);
  }

  draw(list: DrawList, frame: Frame, P: Palette) {
    if (this.tools.front === 'wedge') this.drawPlow(list, frame, P);
    for (const m of this.tanks) this.drawTank(list, frame, P, m);
    for (const c of this.claws) {
      this.drawClaw(list, frame, P, c);
      if (this.powered) this.drawHose(list, frame, P, c);
    }
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

  /**
   * Idle: snap shut, hold, then open and retract, on a loop. In a fight the
   * claw waits open until snap() or grip(). Unpowered: the jaws hang limp.
   */
  private updateClaw(c: Claw, dt: number, time: number, frame: Frame) {
    let open = 0.85, ext = 0, droop = 0.5 + Math.sin(time * 1.3) * 0.05, rate = 3;
    if (this.powered) {
      droop = Math.sin(time * 2) * 0.03;
      let shut: boolean;
      if (this.manual) {
        shut = c.grip || time - c.snapAt < 0.35;
        ext = shut ? 1 : 0.35;
      } else {
        const phase = (time % CLAW_PERIOD) / CLAW_PERIOD;
        shut = phase < 0.3;
        ext = shut ? 1 : 0;
        if (phase < c.phase) c.hiss = true;
        c.phase = phase;
      }
      open = shut ? 0.05 : 1;
      rate = shut ? 30 : 4;
    }
    if (c.hiss) {
      c.hiss = false;
      this.hiss(frame, c);
    }
    c.open = mix(c.open, open, damp(rate, dt));
    c.ext = mix(c.ext, ext, damp(rate, dt));
    c.droop = mix(c.droop, droop, damp(4, dt));
  }

  private hiss(frame: Frame, c: Claw) {
    const at = [toWorld(frame, addScaled(c.m.p, c.m.aim, 0.3))];
    for (const t of this.tanks) at.push(toWorld(frame, this.tankOutlet(t)));
    for (const p of at) {
      for (let i = 0; i < 3; i++) {
        const a = Math.random() * Math.PI * 2;
        this.puffs.push({ p, v: [Math.cos(a) * 0.5, 0.4 + Math.random() * 0.4, Math.sin(a) * 0.5], life: 0, max: 0.3 + Math.random() * 0.2 });
      }
    }
  }

  private drawClaw(list: DrawList, frame: Frame, P: Palette, c: Claw) {
    const cf = compose(frame, c.m.p, mountQ(c.m));
    const W = (v: V3) => toWorld(cf, v);
    const right = qRotate(cf.q, [1, 0, 0]);
    const F = c.base;
    if (F[1] > 0) list.addLocal(cf, Prim.Box, [0, 0.05, 0], qIdentity(), [0.24, 0.1, 0.24], P.dark);
    else list.addLocal(cf, Prim.Box, [0, 0, 0.03], qIdentity(), [0.28, 0.22, 0.07], P.dark);
    const d: V3 = [0, -Math.sin(c.droop), Math.cos(c.droop)];
    const reach = 0.24 + c.ext * 0.18;
    list.seg(Prim.Cyl, W(F), W(addScaled(F, d, 0.22)), [0.11, 1, 0.11], P.main, right);
    list.seg(Prim.Cyl, W(addScaled(F, d, 0.15)), W(addScaled(F, d, reach)), [0.06, 1, 0.06], P.second, right);
    const C = addScaled(F, d, reach);
    list.add(Prim.Box, W(C), qMul(cf.q, qAxisAngle([1, 0, 0], c.droop)), [0.22, 0.12, 0.08], P.accent);
    const a = mix(0.08, 0.65, c.open);
    for (const s of [-1, 1]) {
      const base = addScaled(C, [1, 0, 0], s * 0.07);
      const knee = addScaled(base, norm(addScaled(d, [1, 0, 0], s * Math.tan(a))), 0.2);
      const tip = addScaled(knee, norm(addScaled(d, [1, 0, 0], -s * 0.45)), 0.16);
      list.seg(Prim.Box, W(base), W(knee), [0.06, 1, 0.08], P.second, right);
      list.seg(Prim.Box, W(knee), W(tip), [0.05, 1, 0.07], P.dark, right);
      list.add(Prim.Sphere, W(knee), qIdentity(), [0.07, 0.07, 0.07], P.dark);
    }
    // Air indicator: green when a tank is plumbed in, blinking red when not.
    const lit = this.powered || Math.floor(performance.now() / 400) % 2 === 0;
    const col: V3 = this.powered ? [0.45, 1, 0.4] : lit ? [1, 0.25, 0.2] : [0.35, 0.12, 0.1];
    list.addLocal(cf, Prim.Sphere, add(F, [0.09, 0.12, 0.04]), qIdentity(), [0.06, 0.06, 0.06], col, Mat.Emissive);
  }

  // ---------------------------------------------------------------- pneumatics

  private tankCentre(m: Mount): V3 {
    return add(m.p, [0, TANK_R + 0.04, 0]);
  }

  private tankOutlet(m: Mount): V3 {
    return add(this.tankCentre(m), [0.07, 0.06, TANK_LEN / 2 + 0.06]);
  }

  private drawTank(list: DrawList, frame: Frame, P: Palette, m: Mount) {
    const c = this.tankCentre(m);
    const d = TANK_R * 2;
    list.addLocal(frame, Prim.Cyl, c, X_TO_Z, [d, TANK_LEN, d], P.accent);
    for (const z of [-1, 1]) list.addLocal(frame, Prim.Sphere, add(c, [0, 0, z * TANK_LEN / 2]), qIdentity(), [d, d, TANK_R * 0.9], P.accent);
    for (const z of [-0.12, 0.12]) {
      list.addLocal(frame, Prim.Cyl, add(c, [0, 0, z]), X_TO_Z, [d + 0.03, 0.04, d + 0.03], P.dark);
      list.addLocal(frame, Prim.Box, add(m.p, [0, 0.03, z]), qIdentity(), [0.26, 0.06, 0.05], P.dark);
    }
    // Pressure gauge on top: glows while it's feeding a pincer.
    const feeding = this.claws.length > 0;
    list.addLocal(frame, Prim.Cyl, add(c, [-0.04, TANK_R + 0.02, 0.05]), qIdentity(), [0.12, 0.04, 0.12], P.second);
    list.addLocal(frame, Prim.Cyl, add(c, [-0.04, TANK_R + 0.04, 0.05]), qIdentity(), [0.07, 0.02, 0.07], feeding ? P.glow : P.dark, feeding ? Mat.Emissive : Mat.Lit);
    const o = this.tankOutlet(m);
    list.addLocal(frame, Prim.Cyl, add(o, [0, 0, -0.04]), X_TO_Z, [0.06, 0.1, 0.06], P.dark);
  }

  /** A rubber hose from the first tank, up over the hull, to the pincer's base. */
  private drawHose(list: DrawList, frame: Frame, P: Palette, c: Claw) {
    const { W, H } = this.dims;
    const p0 = this.tankOutlet(this.tanks[0]);
    // Meet the pincer just beside and above its arm.
    const p3 = add(c.m.p, qRotate(mountQ(c.m), add(c.base, [0.1, 0.08, 0])));
    const lift = H / 2 + 0.2;
    const p1: V3 = [W * 0.32, Math.max(p0[1], lift), p0[2] + 0.2 * Math.sign(p3[2] - p0[2] || 1)];
    const p2: V3 = [W * 0.34, Math.max(p3[1], lift), p3[2]];
    const bez = (t: number): V3 => {
      const a = lerp(p0, p1, t), b = lerp(p1, p2, t), cc = lerp(p2, p3, t);
      return lerp(lerp(a, b, t), lerp(b, cc, t), t);
    };
    const right = qRotate(frame.q, [1, 0, 0]);
    const n = 12;
    for (let i = 0; i < n; i++) {
      list.seg(Prim.Cyl, toWorld(frame, bez(i / n)), toWorld(frame, bez((i + 1) / n)), [0.07, 1.15, 0.07], P.dark, right);
    }
  }
}
