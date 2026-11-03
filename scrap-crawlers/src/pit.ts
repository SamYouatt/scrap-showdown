// The fighting pit: a square steel platform raised above a junk-strewn pit
// floor. Walk or get shoved off the edge and you're out.
import { DrawList, Mat } from './render/drawlist';
import { MeshBuilder, Prim } from './render/mesh';
import { Quat, V3, hex, qAxisAngle, qIdentity, qMul, rng, valueNoise } from './math';
import type { Terrain } from './world';

const PLATE_A = hex('#7a8186');
const PLATE_B = hex('#6c7378');
const GRAVEL = hex('#4a4340');
const PAINT = hex('#d9ae3a');
const DARK = hex('#2a2a2d');

interface Piece {
  prim: Prim;
  p: V3;
  q: Quat;
  s: V3;
  col: V3;
  mat?: Mat;
}

export class Pit implements Terrain {
  readonly half = 20;
  /** Half-width of the platform. */
  readonly size = 5;
  readonly drop = 2.2;
  readonly mesh: Float32Array;
  private pieces: Piece[] = [];

  constructor() {
    this.mesh = this.buildMesh();
    this.furnish();
  }

  heightAt(x: number, z: number) {
    return this.onPlatform(x, z) ? 0 : -this.drop;
  }

  onPlatform(x: number, z: number, margin = 0) {
    return Math.abs(x) < this.size + margin && Math.abs(z) < this.size + margin;
  }

  /** Checkerboard deck plates on top, and gravel down in the pit. */
  private buildMesh() {
    const m = new MeshBuilder();
    const S = this.size;
    for (let i = -S; i < S; i++) {
      for (let j = -S; j < S; j++) {
        const col = (i + j) & 1 ? PLATE_A : PLATE_B;
        m.quad([i, 0, j], [i, 0, j + 1], [i + 1, 0, j + 1], [i + 1, 0, j], col, [0, -1, 0]);
      }
    }
    const y = -this.drop, c = 1;
    for (let x = -this.half; x < this.half; x += c) {
      for (let z = -this.half; z < this.half; z += c) {
        if (Math.abs(x + 0.5) < S && Math.abs(z + 0.5) < S) continue;
        const k = 0.85 + valueNoise(x * 0.7, z * 0.7) * 0.3;
        const col: V3 = [GRAVEL[0] * k, GRAVEL[1] * k, GRAVEL[2] * k];
        m.quad([x, y, z], [x, y, z + c], [x + c, y, z + c], [x + c, y, z], col, [0, y - 1, 0]);
      }
    }
    return m.build();
  }

  private add(prim: Prim, p: V3, s: V3, col: V3, q: Quat = qIdentity(), mat?: Mat) {
    this.pieces.push({ prim, p, q, s, col, mat });
  }

  private furnish() {
    const S = this.size, D = this.drop;
    // Platform walls down to the pit floor.
    this.add(Prim.Box, [0, -D / 2 - 0.01, 0], [S * 2, D, S * 2], hex('#4d5357'));
    for (const s of [-1, 1]) {
      this.add(Prim.Box, [0, -0.25, s * S], [S * 2 + 0.04, 0.12, 0.04], DARK);
      this.add(Prim.Box, [s * S, -0.25, 0], [0.04, 0.12, S * 2 + 0.04], DARK);
    }
    // Hazard stripes around the edge of the deck.
    const n = 20;
    for (let i = 0; i < n; i++) {
      const t = -S + (i + 0.5) * (2 * S / n), col = i % 2 ? DARK : PAINT;
      for (const s of [-1, 1]) {
        this.add(Prim.Box, [t, 0.006, s * (S - 0.15)], [2 * S / n, 0.012, 0.3], col);
        this.add(Prim.Box, [s * (S - 0.15), 0.006, t], [0.3, 0.012, 2 * S / n], col);
      }
    }
    // Starting marks.
    for (const s of [-1, 1]) this.add(Prim.Box, [s * 3, 0.006, 0], [0.12, 0.012, 1.2], PAINT);
    // Corner posts with warning lights.
    for (const x of [-1, 1]) {
      for (const z of [-1, 1]) {
        this.add(Prim.Cyl, [x * S, 0.2, z * S], [0.22, 0.4, 0.22], DARK);
        this.add(Prim.Sphere, [x * S, 0.44, z * S], [0.16, 0.16, 0.16], hex('#ff5a3d'), qIdentity(), Mat.Emissive);
      }
    }
    // Floodlight towers well outside the platform.
    for (const [x, z] of [[-9, -9], [9, -9], [-9, 9], [9, 9]]) {
      this.add(Prim.Box, [x, 1 - D, z], [0.25, 6.4, 0.25], DARK);
      this.add(Prim.Box, [x, 4.3 - D, z], [0.9, 0.5, 0.4], hex('#3b3d42'), qAxisAngle([0, 1, 0], Math.atan2(-x, -z)));
      this.add(Prim.Box, [x * 0.97, 4.3 - D, z * 0.97], [0.7, 0.35, 0.1], hex('#fff1b8'), qAxisAngle([0, 1, 0], Math.atan2(-x, -z)), Mat.Emissive);
    }
    // Wrecks of earlier losers down in the pit.
    const r = rng(9);
    const rust = ['#8f5634', '#6d4a33', '#5f7f86', '#4f6f45', '#3b3533'].map(hex);
    for (let i = 0; i < 90; i++) {
      const a = r.range(0, Math.PI * 2), d = r.range(S + 0.8, 14);
      const x = Math.cos(a) * d, z = Math.sin(a) * d;
      if (Math.abs(x) < S + 0.4 && Math.abs(z) < S + 0.4) continue;
      const q = qMul(qAxisAngle([0, 1, 0], r.range(0, 6.3)), qAxisAngle([1, 0, 0], r.range(-0.4, 0.4)));
      const col = r.pick(rust), k = r.int(0, 3), y = -D;
      if (k === 0) this.add(Prim.Box, [x, y + 0.15, z], [r.range(0.3, 0.8), r.range(0.15, 0.35), r.range(0.3, 0.7)], col, q);
      else if (k === 1) this.add(Prim.Cyl, [x, y + 0.08, z], [0.1, r.range(0.5, 1.1), 0.1], hex('#6d7176'), qMul(q, qAxisAngle([0, 0, 1], Math.PI / 2)));
      else if (k === 2) this.add(Prim.Wedge, [x, y + 0.1, z], [0.4, 0.2, 0.45], col, q);
      else this.add(Prim.Cyl, [x, y + 0.12, z], [0.55, 0.22, 0.55], hex('#2c2a2b'));
    }
  }

  draw(list: DrawList) {
    list.add(Prim.Terrain, [0, 0, 0], qIdentity(), [1, 1, 1], [1, 1, 1]);
    for (const p of this.pieces) list.add(p.prim, p.p, p.q, p.s, p.col, p.mat);
  }
}
