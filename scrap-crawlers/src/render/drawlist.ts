// Per-frame list of primitive instances, grouped by primitive for instanced draws.
import { Frame, Quat, V3, add, qMul, qRotate } from '../math';
import { PRIM_COUNT, Prim } from './mesh';

export const INSTANCE_FLOATS = 16;

/** Material flags understood by scene.wgsl. */
export enum Mat {
  Lit = 0,
  Emissive = 1,
  Ground = 2,
}

export class DrawList {
  private lists: number[][] = Array.from({ length: PRIM_COUNT }, () => []);

  clear() {
    for (const l of this.lists) l.length = 0;
  }

  add(prim: Prim, p: V3, q: Quat, s: V3, col: V3, mat: Mat = Mat.Lit) {
    this.lists[prim].push(p[0], p[1], p[2], mat, q[0], q[1], q[2], q[3], s[0], s[1], s[2], 0, col[0], col[1], col[2], 1);
  }

  /** Adds a primitive placed in a parent frame's local space. */
  addLocal(f: Frame, prim: Prim, p: V3, q: Quat, s: V3, col: V3, mat: Mat = Mat.Lit) {
    this.add(prim, add(f.p, qRotate(f.q, p)), qMul(f.q, q), s, col, mat);
  }

  /** Packs every instance into one array and reports the [first, count] range per primitive. */
  pack(): { data: Float32Array; ranges: [number, number][] } {
    let total = 0;
    for (const l of this.lists) total += l.length;
    const data = new Float32Array(Math.max(total, INSTANCE_FLOATS));
    const ranges: [number, number][] = [];
    let offset = 0;
    for (const l of this.lists) {
      data.set(l, offset);
      ranges.push([offset / INSTANCE_FLOATS, l.length / INSTANCE_FLOATS]);
      offset += l.length;
    }
    return { data, ranges };
  }
}
