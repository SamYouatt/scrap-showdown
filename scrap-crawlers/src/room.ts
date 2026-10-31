// The workshop: a concrete floor with a turntable in the middle and low clutter
// around the edges. Everything near the turntable stays short so the orbiting
// camera never has a prop between it and the build.
import { DrawList } from './render/drawlist';
import { MeshBuilder, Prim } from './render/mesh';
import { Quat, V3, fbm, hex, mix, qAxisAngle, qIdentity, qMul, rng, valueNoise } from './math';
import type { Terrain } from './world';

const CONCRETE = hex('#8a857c');
const SEAM = hex('#5f5b55');
const OIL = hex('#4c463f');
const PAINT = hex('#d9ae3a');
const STEEL = hex('#7c848a');
const DARK = hex('#2f3033');

interface Piece {
  prim: Prim;
  p: V3;
  q: Quat;
  s: V3;
  col: V3;
}

export class WorkshopRoom implements Terrain {
  readonly half = 24;
  readonly turntable = 2.1;
  readonly deck = 0.1;
  readonly mesh: Float32Array;
  private pieces: Piece[] = [];

  constructor() {
    this.mesh = this.buildFloor();
    this.furnish();
  }

  heightAt(x: number, z: number) {
    return x * x + z * z < this.turntable * this.turntable ? this.deck : 0;
  }

  /** Concrete slabs, two metres square, with darker seams and oil stains. */
  private buildFloor() {
    const m = new MeshBuilder();
    const cell = 0.5, n = (this.half * 2) / cell, seam = 0.03;
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        const x0 = -this.half + i * cell, z0 = -this.half + j * cell;
        const slabX = Math.floor(i / 4), slabZ = Math.floor(j / 4);
        const shade = 0.93 + valueNoise(slabX * 7.1, slabZ * 3.3) * 0.12 + (valueNoise(i * 1.7, j * 2.3) - 0.5) * 0.04;
        let col: V3 = [CONCRETE[0] * shade, CONCRETE[1] * shade, CONCRETE[2] * shade];
        const oil = fbm(x0 * 0.3 + 11, z0 * 0.3 - 4, 3);
        if (oil > 0.64) col = mixV(col, OIL, 0.6);
        const q = (a: number, b: number, c: number, d: number, k: V3) =>
          m.quad([a, 0, b], [a, 0, d], [c, 0, d], [c, 0, b], k, [0, -1, 0]);
        // Slab seams run along every fourth cell edge.
        const sx = i % 4 === 0 ? seam : 0, sz = j % 4 === 0 ? seam : 0;
        q(x0 + sx, z0 + sz, x0 + cell, z0 + cell, col);
        if (sx) q(x0, z0, x0 + sx, z0 + cell, SEAM);
        if (sz) q(x0 + sx, z0, x0 + cell, z0 + sz, SEAM);
      }
    }
    return m.build();
  }

  private add(prim: Prim, p: V3, s: V3, col: V3, q: Quat = qIdentity()) {
    this.pieces.push({ prim, p, q, s, col });
  }

  private furnish() {
    const R = this.turntable, y = this.deck;
    // Turntable: steel deck, dark rim, hazard ring and a centre boss.
    this.add(Prim.Cyl, [0, y / 2, 0], [R * 2, y, R * 2], STEEL);
    this.add(Prim.Cyl, [0, 0.03, 0], [R * 2 + 0.12, 0.06, R * 2 + 0.12], DARK);
    const stripes = 36;
    for (let i = 0; i < stripes; i++) {
      const a = (i / stripes) * Math.PI * 2;
      const q = qAxisAngle([0, 1, 0], -a);
      this.add(Prim.Box, [Math.cos(a) * (R - 0.14), y + 0.005, Math.sin(a) * (R - 0.14)], [0.16, 0.012, ((Math.PI * 2 * R) / stripes) * 0.95], i % 2 ? DARK : PAINT, q);
    }
    for (let i = 0; i < 8; i++) {
      const a = (i / 8) * Math.PI * 2;
      this.add(Prim.Cyl, [Math.cos(a) * (R - 0.4), y + 0.01, Math.sin(a) * (R - 0.4)], [0.08, 0.02, 0.08], DARK);
    }

    // Painted bay lines around the turntable.
    const b = 3.2;
    for (const s of [-1, 1]) {
      this.add(Prim.Box, [0, 0.004, s * b], [b * 2 + 0.12, 0.008, 0.12], PAINT);
      this.add(Prim.Box, [s * b, 0.004, 0], [0.12, 0.008, b * 2 + 0.12], PAINT);
    }

    // Low props around the edges.
    this.toolChest([5.2, 0, -2.2], 0.5);
    this.workbench([-4.9, 0, -2.8], -0.4);
    this.tyres([-5.2, 0, 2.6]);
    this.drums([2.4, 0, 5.4]);
    this.scrapBin([5.4, 0, 2.9], 0.3);
    this.pallet([-1.6, 0, -5.6], 0.15);
    this.gasBottle([6.4, 0, -4.4]);

    // Floor clutter outside the bay.
    const r = rng(21);
    const bits = ['#8d8f95', '#a3593a', '#c9a23a', '#5f7f86', '#3b3533'].map(hex);
    for (let i = 0; i < 70; i++) {
      const a = r.range(0, Math.PI * 2), d = r.range(4.2, 11);
      const x = Math.cos(a) * d, z = Math.sin(a) * d;
      const q = qAxisAngle([0, 1, 0], r.range(0, Math.PI * 2));
      const k = r.int(0, 3), col = r.pick(bits);
      if (k === 0) this.add(Prim.Cyl, [x, 0.02, z], [0.1, 0.04, 0.1], col, q);
      else if (k === 1) this.add(Prim.Box, [x, 0.015, z], [r.range(0.2, 0.4), 0.03, 0.06], hex('#9a9da2'), q);
      else if (k === 2) this.add(Prim.Wedge, [x, 0.04, z], [0.16, 0.08, 0.18], col, q);
      else this.add(Prim.Cyl, [x, 0.03, z], [0.05, r.range(0.4, 0.8), 0.05], hex('#6d7176'), qMul(q, qAxisAngle([0, 0, 1], Math.PI / 2)));
    }
  }

  private local(o: V3, yaw: number, p: V3): V3 {
    const c = Math.cos(yaw), s = Math.sin(yaw);
    return [o[0] + c * p[0] + s * p[2], o[1] + p[1], o[2] - s * p[0] + c * p[2]];
  }

  private toolChest(o: V3, yaw: number) {
    const q = qAxisAngle([0, 1, 0], yaw), L = (p: V3) => this.local(o, yaw, p);
    const red = hex('#b23b2c');
    this.add(Prim.Box, L([0, 0.5, 0]), [1.3, 0.9, 0.6], red, q);
    this.add(Prim.Box, L([0, 0.03, 0]), [1.34, 0.06, 0.64], DARK, q);
    for (let i = 0; i < 4; i++) {
      this.add(Prim.Box, L([0, 0.2 + i * 0.2, 0.305]), [1.2, 0.015, 0.01], DARK, q);
      this.add(Prim.Box, L([0, 0.28 + i * 0.2, 0.32]), [0.4, 0.03, 0.03], hex('#c8c9c4'), q);
    }
    this.add(Prim.Box, L([-0.2, 0.98, 0]), [0.5, 0.05, 0.12], hex('#9a9da2'), qMul(q, qAxisAngle([0, 1, 0], 0.3)));
  }

  private workbench(o: V3, yaw: number) {
    const q = qAxisAngle([0, 1, 0], yaw), L = (p: V3) => this.local(o, yaw, p);
    const wood = hex('#9c7148');
    this.add(Prim.Box, L([0, 0.86, 0]), [2.2, 0.08, 0.8], wood, q);
    this.add(Prim.Box, L([0, 0.3, 0]), [2.0, 0.05, 0.7], hex('#7a5839'), q);
    for (const x of [-1, 1]) for (const z of [-1, 1]) this.add(Prim.Box, L([x * 1.0, 0.41, z * 0.33]), [0.08, 0.82, 0.08], DARK, q);
    this.add(Prim.Box, L([0.8, 0.98, 0.3]), [0.2, 0.16, 0.16], hex('#4f6f8f'), q);
    this.add(Prim.Cyl, L([-0.4, 0.95, 0]), [0.3, 0.1, 0.3], STEEL);
    this.add(Prim.Cyl, L([0.1, 0.98, -0.15]), [0.14, 0.16, 0.14], hex('#c9a23a'));
    this.add(Prim.Box, L([-0.6, 0.39, 0.05]), [0.5, 0.14, 0.35], hex('#5f7f86'), q);
  }

  private tyres(o: V3) {
    for (let i = 0; i < 3; i++) {
      this.add(Prim.Cyl, [o[0] + i * 0.04, 0.12 + i * 0.24, o[2] - i * 0.03], [1.1, 0.22, 1.1], hex('#2c2a2b'));
      this.add(Prim.Cyl, [o[0] + i * 0.04, 0.235 + i * 0.24, o[2] - i * 0.03], [0.6, 0.01, 0.6], hex('#45403c'));
    }
  }

  private drums(o: V3) {
    this.add(Prim.Cyl, [o[0], 0.45, o[2]], [0.7, 0.9, 0.7], hex('#3f6f8f'));
    for (const y of [0.3, 0.62]) this.add(Prim.Cyl, [o[0], y, o[2]], [0.74, 0.05, 0.74], hex('#2a4a60'));
    const lie = qAxisAngle([0, 0, 1], Math.PI / 2);
    this.add(Prim.Cyl, [o[0] + 1.0, 0.35, o[2] + 0.3], [0.7, 0.9, 0.7], hex('#b8462e'), qMul(qAxisAngle([0, 1, 0], 0.4), lie));
    this.add(Prim.Cyl, [o[0] + 1.5, 0.004, o[2] - 0.2], [1.2, 0.008, 0.8], OIL);
  }

  private scrapBin(o: V3, yaw: number) {
    const q = qAxisAngle([0, 1, 0], yaw), L = (p: V3) => this.local(o, yaw, p);
    const green = hex('#5d6b3e');
    this.add(Prim.Box, L([0, 0.35, 0]), [1.1, 0.7, 0.8], green, q);
    this.add(Prim.Box, L([0, 0.69, 0]), [0.98, 0.04, 0.68], DARK, q);
    this.add(Prim.Cyl, L([0.2, 0.85, 0.1]), [0.08, 0.6, 0.08], hex('#8d8f95'), qMul(q, qAxisAngle([0, 0, 1], 0.5)));
    this.add(Prim.Box, L([-0.25, 0.78, -0.1]), [0.35, 0.25, 0.04], hex('#a3593a'), qMul(q, qAxisAngle([1, 0, 0], 0.4)));
    this.add(Prim.Cyl, L([-0.1, 0.8, 0.15]), [0.25, 0.2, 0.25], hex('#c9a23a'));
  }

  private pallet(o: V3, yaw: number) {
    const q = qAxisAngle([0, 1, 0], yaw), L = (p: V3) => this.local(o, yaw, p);
    const wood = hex('#a07f55');
    for (const z of [-0.45, 0, 0.45]) this.add(Prim.Box, L([0, 0.05, z]), [1.2, 0.1, 0.12], wood, q);
    this.add(Prim.Box, L([0, 0.13, 0]), [1.2, 0.04, 1.0], wood, q);
    // An old engine block waiting to be stripped.
    this.add(Prim.Box, L([0, 0.42, 0]), [0.8, 0.54, 0.6], hex('#5a5f63'), q);
    this.add(Prim.Box, L([0, 0.73, 0]), [0.6, 0.08, 0.5], hex('#3e4246'), q);
    for (const x of [-0.25, 0, 0.25]) this.add(Prim.Cyl, L([x, 0.8, 0]), [0.12, 0.08, 0.12], STEEL);
  }

  private gasBottle(o: V3) {
    this.add(Prim.Cyl, [o[0], 0.6, o[2]], [0.34, 1.2, 0.34], hex('#4f6f45'));
    this.add(Prim.Sphere, [o[0], 1.2, o[2]], [0.34, 0.2, 0.34], hex('#4f6f45'));
    this.add(Prim.Cyl, [o[0], 1.33, o[2]], [0.08, 0.1, 0.08], hex('#c9a23a'));
  }

  draw(list: DrawList) {
    list.add(Prim.Terrain, [0, 0, 0], qIdentity(), [1, 1, 1], [1, 1, 1]);
    for (const p of this.pieces) list.add(p.prim, p.p, p.q, p.s, p.col);
  }
}

const mixV = (a: V3, b: V3, t: number): V3 => [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)];
