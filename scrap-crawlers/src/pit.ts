// The fighting pit: a square platform raised above the pit floor. Walk or get
// shoved off the edge and you're out. The rules only need the platform; its look
// is a theme, so each part of the world can have arenas that fit in:
//  - steel: the arena page's plated deck with hazard stripes and floodlights
//  - yard: a packed-dirt mound in the scrapyard, walled with tyres and lit by burning drums
import { DrawList, Mat } from './render/drawlist';
import { MeshBuilder, Prim } from './render/mesh';
import { Quat, V3, fbm, hex, mix, qAxisAngle, qIdentity, qMul, rng, valueNoise } from './math';
import type { Terrain } from './world';

const PLATE_A = hex('#7a8186');
const PLATE_B = hex('#6c7378');
const GRAVEL = hex('#4a4340');
const PAINT = hex('#d9ae3a');
const DARK = hex('#2a2a2d');
// The scrapyard's ground colours (see world.ts), for the yard theme.
const DIRT = hex('#7d6a4c');
const SAND = hex('#a08b62');
const OIL = hex('#4b4236');
const RUST = hex('#8f5634');
const MOSS = hex('#6a7444');
const MUD = hex('#4f4334');

export type PitTheme = 'steel' | 'yard';

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

  constructor(readonly theme: PitTheme = 'steel') {
    if (theme === 'yard') {
      this.mesh = this.buildYardMesh();
      this.furnishYard();
    } else {
      this.mesh = this.buildMesh();
      this.furnish();
    }
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

  /** Packed dirt on top, with tyre ruts and oil stains in the scrapyard's colours, and muddy ground below. */
  private buildYardMesh() {
    const m = new MeshBuilder();
    const S = this.size, c = 0.5;
    for (let x = -S; x < S; x += c) {
      for (let z = -S; z < S; z += c) {
        const cx = x + c / 2, cz = z + c / 2;
        let col = mixV(DIRT, SAND, 0.25 + valueNoise(cx * 0.9, cz * 0.9) * 0.3);
        // Two worn ruts across the middle where crawlers have shoved each other back and forth.
        if (Math.abs(Math.abs(cz) - 1.2) < 0.3 && Math.abs(cx) < S - 0.6) col = mixV(col, OIL, 0.45);
        if (fbm(cx * 0.4 + 9, cz * 0.4 - 3, 3) > 0.62) col = mixV(col, OIL, 0.75);
        else if (fbm(cx * 0.5 - 20, cz * 0.5 + 6, 3) > 0.64) col = mixV(col, RUST, 0.6);
        m.quad([x, 0, z], [x, 0, z + c], [x + c, 0, z + c], [x + c, 0, z], col, [0, -1, 0]);
      }
    }
    const y = -this.drop, f = 1;
    for (let x = -this.half; x < this.half; x += f) {
      for (let z = -this.half; z < this.half; z += f) {
        if (Math.abs(x + 0.5) < S && Math.abs(z + 0.5) < S) continue;
        let col = mixV(DIRT, MUD, 0.5 + valueNoise(x * 0.5, z * 0.5) * 0.4);
        if (fbm(x * 0.15 + 4, z * 0.15, 3) > 0.6) col = mixV(col, MOSS, 0.6);
        m.quad([x, y, z], [x, y, z + f], [x + f, y, z + f], [x + f, y, z], col, [0, y - 1, 0]);
      }
    }
    return m.build();
  }

  private furnishYard() {
    const S = this.size, D = this.drop;
    const r = rng(17);
    // The mound itself, then a retaining wall of tyres stacked up each face.
    this.add(Prim.Box, [0, -D / 2 - 0.01, 0], [S * 2, D, S * 2], hex('#5e4f3a'));
    const tyre = hex('#2c2a2b'), hub = hex('#45403c');
    const along = 14;
    for (let side = 0; side < 4; side++) {
      const q = qAxisAngle([0, 1, 0], (side * Math.PI) / 2);
      for (let i = 0; i < along; i++) {
        const t = -S + 0.55 + (i * (2 * S - 1.1)) / (along - 1);
        for (let k = 0; k < 3; k++) {
          // Each row sits a little proud of the face and staggers, like tyres actually stacked by hand.
          const off = (k % 2) * 0.32 + r.range(-0.05, 0.05);
          const p = rot(side, [t + off - 0.16, -D + 0.32 + k * 0.62, S + 0.3]);
          this.add(Prim.Cyl, p, [0.64, 0.4, 0.64], tyre, qMul(q, qAxisAngle([1, 0, 0], Math.PI / 2)));
          this.add(Prim.Cyl, rot(side, [t + off - 0.16, -D + 0.32 + k * 0.62, S + 0.51]), [0.34, 0.02, 0.34], hub, qMul(q, qAxisAngle([1, 0, 0], Math.PI / 2)));
        }
      }
      // A rim of half-buried tyres marks the edge of the deck.
      for (let i = 0; i < 12; i++) {
        const t = -S + (i + 0.5) * (2 * S / 12);
        this.add(Prim.Cyl, rot(side, [t, -0.12, S + 0.12]), [0.6, 0.22, 0.6], tyre, qMul(q, qAxisAngle([1, 0, 0], Math.PI / 2)));
      }
    }
    // Starting marks: a scrap plank at each end.
    for (const s of [-1, 1]) this.add(Prim.Box, [s * 3, 0.02, 0], [0.22, 0.04, 1.3], hex('#a07f55'), qAxisAngle([0, 1, 0], s * 0.06));
    // Burning drums on the corners light the fight.
    for (const x of [-1, 1]) {
      for (const z of [-1, 1]) {
        const p: V3 = [x * (S + 0.7), -D, z * (S + 0.7)];
        this.add(Prim.Cyl, [p[0], p[1] + 0.45, p[2]], [0.7, 0.9, 0.7], hex('#b8462e'));
        this.add(Prim.Cyl, [p[0], p[1] + 0.62, p[2]], [0.74, 0.05, 0.74], hex('#7a2c1d'));
        this.add(Prim.Cone, [p[0], p[1] + 1.1, p[2]], [0.5, 0.5, 0.5], hex('#ff9a2e'), qIdentity(), Mat.Emissive);
        this.add(Prim.Cone, [p[0], p[1] + 1.05, p[2]], [0.3, 0.75, 0.3], hex('#ffd65a'), qIdentity(), Mat.Emissive);
      }
    }
    // Lamps strung on scrap-pipe poles.
    for (const [x, z] of [[-8, -8], [8, -8], [-8, 8], [8, 8]]) {
      this.add(Prim.Cyl, [x, 1.4 - D, z], [0.16, 6.8, 0.16], hex('#6d7176'));
      this.add(Prim.Cyl, [x * 0.97, 4.6 - D, z * 0.97], [0.5, 0.3, 0.5], hex('#3b3d42'));
      this.add(Prim.Sphere, [x * 0.97, 4.42 - D, z * 0.97], [0.3, 0.3, 0.3], hex('#fff1b8'), qIdentity(), Mat.Emissive);
    }
    // The yard crowds in around the hollow: stacked crates, drums and tyre piles.
    const crateCols = ['#9a7449', '#7f8a8c', '#5f7f86', '#a3593a', '#8a8450'].map(hex);
    const drumCols = ['#b8462e', '#3f6f8f', '#c9a23a', '#4f6f45'].map(hex);
    for (let i = 0; i < 70; i++) {
      const a = r.range(0, Math.PI * 2), d = r.range(S + 2.4, 15);
      const x = Math.cos(a) * d, z = Math.sin(a) * d;
      if (Math.abs(x) < S + 2 && Math.abs(z) < S + 2) continue;
      const yaw = qAxisAngle([0, 1, 0], r.range(0, 6.3)), k = r.int(0, 3), y = -D;
      if (k === 0) {
        const h = r.range(0.5, 1.1), w = r.range(0.7, 1.5);
        this.add(Prim.Box, [x, y + h / 2, z], [w, h, r.range(0.7, 1.4)], r.pick(crateCols), yaw);
        if (r.chance(0.5)) this.add(Prim.Box, [x, y + h + 0.25, z], [w * 0.6, 0.5, 0.6], r.pick(crateCols), qMul(yaw, qAxisAngle([0, 1, 0], 0.4)));
      } else if (k === 1) {
        this.add(Prim.Cyl, [x, y + 0.45, z], [0.65, 0.9, 0.65], r.pick(drumCols));
      } else if (k === 2) {
        for (let j = 0; j < r.int(1, 3); j++) this.add(Prim.Cyl, [x, y + 0.12 + j * 0.24, z], [1.1, 0.22, 1.1], tyre);
      } else {
        this.add(Prim.Cyl, [x, y + 0.08, z], [0.1, r.range(0.6, 1.4), 0.1], hex('#6d7176'), qMul(yaw, qAxisAngle([0, 0, 1], Math.PI / 2)));
      }
    }
  }

  draw(list: DrawList) {
    list.add(Prim.Terrain, [0, 0, 0], qIdentity(), [1, 1, 1], [1, 1, 1], this.theme === 'yard' ? Mat.Ground : Mat.Lit);
    for (const p of this.pieces) list.add(p.prim, p.p, p.q, p.s, p.col, p.mat);
  }
}

const mixV = (a: V3, b: V3, t: number): V3 => [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)];

/** A point given for the +z face, turned to face `side` (0..3, quarter turns about y). */
function rot(side: number, p: V3): V3 {
  const a = (side * Math.PI) / 2, c = Math.cos(a), s = Math.sin(a);
  return [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]];
}
