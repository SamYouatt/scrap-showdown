// The scrapyard: a rolling heightfield plus walkable crates, drums and tyres.
// heightAt() is the single source of truth for feet, the body and mouse picks.
import { DrawList, Mat } from './render/drawlist';
import { MeshBuilder, Prim } from './render/mesh';
import { Quat, Rng, V3, add, fbm, hex, mix, qAxisAngle, qIdentity, qMul, rng, valueNoise } from './math';

interface Obstacle {
  kind: 'crate' | 'drum' | 'tyre';
  x: number;
  z: number;
  /** Half extents for crates, radius in hx for drums and tyres. */
  hx: number;
  hz: number;
  yaw: number;
  base: number;
  top: number;
  col: V3;
}

interface Debris {
  prim: Prim;
  p: V3;
  q: Quat;
  s: V3;
  col: V3;
}

const DIRT = hex('#7d6a4c');
const OIL = hex('#4b4236');
const RUST = hex('#8f5634');
const MOSS = hex('#6a7444');
const SAND = hex('#a08b62');

export class World {
  readonly half = 32;
  readonly obstacles: Obstacle[] = [];
  private debris: Debris[] = [];
  readonly mesh: Float32Array;

  constructor(seed: number) {
    const r = rng(seed);
    this.placeObstacles(r);
    this.scatterDebris(r);
    this.mesh = this.buildTerrain();
  }

  ground(x: number, z: number) {
    return (fbm(x * 0.05 + 3.1, z * 0.05 - 7.3, 4) - 0.5) * 4.5 + (valueNoise(x * 0.6, z * 0.6) - 0.5) * 0.12;
  }

  heightAt(x: number, z: number) {
    let h = this.ground(x, z);
    for (const o of this.obstacles) {
      if (o.top <= h) continue;
      const dx = x - o.x, dz = z - o.z;
      if (o.kind === 'crate') {
        const c = Math.cos(o.yaw), s = Math.sin(o.yaw);
        const lx = c * dx + s * dz, lz = -s * dx + c * dz;
        if (Math.abs(lx) < o.hx && Math.abs(lz) < o.hz) h = o.top;
      } else if (dx * dx + dz * dz < o.hx * o.hx) {
        h = o.top;
      }
    }
    return h;
  }

  /** First hit of a ray with the walkable surface. */
  raycast(origin: V3, dir: V3): V3 | null {
    let prev = 0;
    for (let t = 0; t < 220; t += 0.2) {
      const p = add(origin, [dir[0] * t, dir[1] * t, dir[2] * t]);
      if (p[1] <= this.heightAt(p[0], p[2])) {
        let lo = prev, hi = t;
        for (let i = 0; i < 16; i++) {
          const m = (lo + hi) / 2;
          const q = add(origin, [dir[0] * m, dir[1] * m, dir[2] * m]);
          if (q[1] <= this.heightAt(q[0], q[2])) hi = m;
          else lo = m;
        }
        const hit = add(origin, [dir[0] * hi, dir[1] * hi, dir[2] * hi]);
        if (Math.abs(hit[0]) > this.half || Math.abs(hit[2]) > this.half) return null;
        return hit;
      }
      prev = t;
    }
    return null;
  }

  private footprintMax(x: number, z: number, r: number) {
    let h = -Infinity;
    for (const [ox, oz] of [[0, 0], [r, r], [-r, r], [r, -r], [-r, -r]]) h = Math.max(h, this.heightAt(x + ox, z + oz));
    return h;
  }

  private placeObstacles(r: Rng) {
    const crateCols = ['#9a7449', '#7f8a8c', '#5f7f86', '#a3593a', '#8a8450'].map(hex);
    const drumCols = ['#b8462e', '#3f6f8f', '#c9a23a', '#4f6f45'].map(hex);
    const tryPlace = (near: boolean) => {
      const a = r.range(0, Math.PI * 2);
      const d = near ? r.range(2.6, 6) : r.range(5, this.half - 2);
      const x = Math.cos(a) * d, z = Math.sin(a) * d;
      const kind = r.pick(['crate', 'crate', 'crate', 'drum', 'tyre'] as const);
      if (kind === 'crate') {
        const hx = r.range(0.4, 1.1), hz = r.range(0.4, 1.1);
        const base = this.footprintMax(x, z, Math.max(hx, hz)) - 0.3;
        const ground = this.footprintMax(x, z, Math.max(hx, hz));
        const yaw = r.range(-0.6, 0.6);
        this.obstacles.push({ kind, x, z, hx, hz, yaw, base, top: ground + r.range(0.25, 0.75), col: r.pick(crateCols) });
        if (r.chance(0.4)) {
          // A smaller crate stacked on top makes a little staircase.
          const s = r.range(0.45, 0.7);
          const top = this.obstacles[this.obstacles.length - 1].top;
          this.obstacles.push({
            kind, x: x + r.range(-0.2, 0.2), z: z + r.range(-0.2, 0.2), hx: hx * s, hz: hz * s,
            yaw: yaw + r.range(-0.4, 0.4), base: top, top: top + r.range(0.25, 0.5), col: r.pick(crateCols),
          });
        }
      } else if (kind === 'drum') {
        const rad = r.range(0.3, 0.42);
        const ground = this.footprintMax(x, z, rad);
        this.obstacles.push({ kind, x, z, hx: rad, hz: rad, yaw: 0, base: ground - 0.3, top: ground + r.range(0.6, 0.9), col: r.pick(drumCols) });
      } else {
        const rad = r.range(0.5, 0.7);
        const ground = this.footprintMax(x, z, rad);
        this.obstacles.push({ kind, x, z, hx: rad, hz: rad, yaw: 0, base: ground - 0.2, top: ground + 0.22, col: hex('#2c2a2b') });
      }
    };
    for (let i = 0; i < 7; i++) tryPlace(true);
    for (let i = 0; i < 70; i++) tryPlace(false);
  }

  private scatterDebris(r: Rng) {
    const cols = ['#8d8f95', '#a3593a', '#c9a23a', '#5f7f86', '#3b3533', '#b5b0a2'].map(hex);
    for (let i = 0; i < 320; i++) {
      const x = r.range(-this.half, this.half), z = r.range(-this.half, this.half);
      const y = this.ground(x, z);
      if (this.heightAt(x, z) > y + 0.01) continue;
      const kind = r.int(0, 3);
      const yaw = qAxisAngle([0, 1, 0], r.range(0, Math.PI * 2));
      const col = r.pick(cols);
      if (kind === 0) this.debris.push({ prim: Prim.Box, p: [x, y + 0.01, z], q: yaw, s: [r.range(0.15, 0.45), 0.03, r.range(0.1, 0.3)], col });
      else if (kind === 1) this.debris.push({ prim: Prim.Cyl, p: [x, y + 0.03, z], q: yaw, s: [0.09, 0.06, 0.09], col });
      else if (kind === 2) this.debris.push({ prim: Prim.Wedge, p: [x, y + 0.05, z], q: yaw, s: [0.18, 0.1, 0.2], col });
      else {
        const lie = qAxisAngle([0, 0, 1], Math.PI / 2);
        this.debris.push({ prim: Prim.Cyl, p: [x, y + 0.04, z], q: qMul(yaw, lie), s: [0.07, r.range(0.4, 0.9), 0.07], col: hex('#6d7176') });
      }
    }
  }

  private buildTerrain() {
    const m = new MeshBuilder();
    const n = 96, size = this.half * 2, cell = size / n;
    const p = (i: number, j: number): V3 => {
      const x = -this.half + i * cell, z = -this.half + j * cell;
      return [x, this.ground(x, z), z];
    };
    const colourAt = (c: V3): V3 => {
      const oil = fbm(c[0] * 0.18 + 40, c[2] * 0.18, 3);
      const rust = fbm(c[0] * 0.25 - 11, c[2] * 0.25 + 5, 3);
      const moss = fbm(c[0] * 0.12 + 7, c[2] * 0.12 + 70, 3);
      let col = DIRT;
      col = mixV(col, SAND, clamp01((c[1] + 0.6) * 0.35));
      if (oil > 0.6) col = mixV(col, OIL, 0.8);
      else if (rust > 0.62) col = mixV(col, RUST, 0.7);
      else if (moss > 0.6) col = mixV(col, MOSS, 0.75);
      return col;
    };
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        const a = p(i, j), b = p(i + 1, j), c = p(i + 1, j + 1), d = p(i, j + 1);
        for (const [x, y, z] of [[a, b, c], [a, c, d]]) {
          const cen: V3 = [(x[0] + y[0] + z[0]) / 3, (x[1] + y[1] + z[1]) / 3, (x[2] + y[2] + z[2]) / 3];
          m.tri(x, y, z, colourAt(cen), [cen[0], cen[1] - 10, cen[2]]);
        }
      }
    }
    return m.build();
  }

  draw(list: DrawList) {
    list.add(Prim.Terrain, [0, 0, 0], qIdentity(), [1, 1, 1], [1, 1, 1], Mat.Ground);
    for (const o of this.obstacles) {
      const h = o.top - o.base;
      const c: V3 = [o.x, (o.top + o.base) / 2, o.z];
      if (o.kind === 'crate') {
        const q = qAxisAngle([0, 1, 0], -o.yaw);
        list.add(Prim.Box, c, q, [o.hx * 2, h, o.hz * 2], o.col);
        // Lid and a darker band so crates read as made objects, not blocks.
        list.add(Prim.Box, [o.x, o.top - 0.03, o.z], q, [o.hx * 2 - 0.1, 0.08, o.hz * 2 - 0.1], scaleCol(o.col, 1.12));
        list.add(Prim.Box, [o.x, o.top - h * 0.45, o.z], q, [o.hx * 2 + 0.02, 0.06, o.hz * 2 + 0.02], scaleCol(o.col, 0.7));
      } else if (o.kind === 'drum') {
        const d = o.hx * 2;
        list.add(Prim.Cyl, c, qIdentity(), [d, h, d], o.col);
        for (const f of [0.3, 0.65]) list.add(Prim.Cyl, [o.x, o.base + h * f + 0.1, o.z], qIdentity(), [d + 0.04, 0.05, d + 0.04], scaleCol(o.col, 0.65));
        list.add(Prim.Cyl, [o.x, o.top, o.z], qIdentity(), [d * 0.25, 0.04, d * 0.25], hex('#2b2b2e'));
      } else {
        const d = o.hx * 2;
        list.add(Prim.Cyl, c, qIdentity(), [d, h, d], o.col);
        list.add(Prim.Cyl, [o.x, o.top, o.z], qIdentity(), [d * 0.55, 0.02, d * 0.55], hex('#4a4440'));
      }
    }
    for (const d of this.debris) list.add(d.prim, d.p, d.q, d.s, d.col);
  }
}

const clamp01 = (x: number) => Math.min(1, Math.max(0, x));
const mixV = (a: V3, b: V3, t: number): V3 => [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)];
const scaleCol = (c: V3, s: number): V3 => [Math.min(1, c[0] * s), Math.min(1, c[1] * s), Math.min(1, c[2] * s)];
