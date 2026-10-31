// Flat-shaded primitive meshes. Every vertex is position(3) normal(3) colour(3).
// Scrap parts are assembled from instances of these primitives, so the GPU only
// ever sees a handful of tiny meshes plus the terrain.
import { V3, add, cross, dot, norm, scale, sub } from '../math';

export enum Prim {
  Box,
  Cyl,
  Cone,
  Wedge,
  Sphere,
  Terrain,
}
export const PRIM_COUNT = 6;
export const VERTEX_FLOATS = 9;

export class MeshBuilder {
  data: number[] = [];

  /** Adds a triangle with a flat normal. For convex shapes centred on the origin,
   * pass `outward` so the normal is flipped away from the centre if needed. */
  tri(a: V3, b: V3, c: V3, col: V3 = [1, 1, 1], outward?: V3) {
    let n = norm(cross(sub(b, a), sub(c, a)));
    if (outward && dot(n, sub(scale(add(add(a, b), c), 1 / 3), outward)) < 0) n = scale(n, -1);
    for (const p of [a, b, c]) this.data.push(p[0], p[1], p[2], n[0], n[1], n[2], col[0], col[1], col[2]);
  }
  quad(a: V3, b: V3, c: V3, d: V3, col?: V3, outward?: V3) {
    this.tri(a, b, c, col, outward);
    this.tri(a, c, d, col, outward);
  }
  build() {
    return new Float32Array(this.data);
  }
}

const O: V3 = [0, 0, 0];

function box() {
  const m = new MeshBuilder();
  const h = 0.5;
  const c = (x: number, y: number, z: number): V3 => [x * h, y * h, z * h];
  m.quad(c(1, -1, -1), c(1, 1, -1), c(1, 1, 1), c(1, -1, 1), undefined, O);
  m.quad(c(-1, -1, -1), c(-1, -1, 1), c(-1, 1, 1), c(-1, 1, -1), undefined, O);
  m.quad(c(-1, 1, -1), c(-1, 1, 1), c(1, 1, 1), c(1, 1, -1), undefined, O);
  m.quad(c(-1, -1, -1), c(1, -1, -1), c(1, -1, 1), c(-1, -1, 1), undefined, O);
  m.quad(c(-1, -1, 1), c(1, -1, 1), c(1, 1, 1), c(-1, 1, 1), undefined, O);
  m.quad(c(-1, -1, -1), c(-1, 1, -1), c(1, 1, -1), c(1, -1, -1), undefined, O);
  return m.build();
}

/** Unit-height cylinder (or cone when topRadius is 0) along Y, centred on the origin. */
function cylinder(segments: number, topRadius: number) {
  const m = new MeshBuilder();
  const r = 0.5;
  const ring = (i: number, y: number, rr: number): V3 => {
    const a = (i / segments) * Math.PI * 2;
    return [Math.cos(a) * rr, y, Math.sin(a) * rr];
  };
  for (let i = 0; i < segments; i++) {
    const a0 = ring(i, -0.5, r), a1 = ring(i + 1, -0.5, r);
    const b0 = ring(i, 0.5, topRadius), b1 = ring(i + 1, 0.5, topRadius);
    if (topRadius > 0) m.quad(a0, b0, b1, a1, undefined, O);
    else m.tri(a0, b0, a1, undefined, O);
    m.tri([0, -0.5, 0], a0, a1, undefined, O);
    if (topRadius > 0) m.tri([0, 0.5, 0], b1, b0, undefined, O);
  }
  return m.build();
}

/** Triangular prism: a ramp rising towards -Z, extruded along X. */
function wedge() {
  const m = new MeshBuilder();
  const h = 0.5;
  const centre: V3 = [0, -h / 3, -h / 3];
  const p = (x: number, y: number, z: number): V3 => [x * h, y * h, z * h];
  m.quad(p(-1, -1, -1), p(1, -1, -1), p(1, -1, 1), p(-1, -1, 1), undefined, centre);
  m.quad(p(-1, -1, -1), p(-1, 1, -1), p(1, 1, -1), p(1, -1, -1), undefined, centre);
  m.quad(p(-1, 1, -1), p(-1, -1, 1), p(1, -1, 1), p(1, 1, -1), undefined, centre);
  m.tri(p(-1, -1, -1), p(-1, -1, 1), p(-1, 1, -1), undefined, centre);
  m.tri(p(1, -1, -1), p(1, 1, -1), p(1, -1, 1), undefined, centre);
  return m.build();
}

/** Low-poly icosphere (one subdivision), radius 0.5. */
function sphere() {
  const t = (1 + Math.sqrt(5)) / 2;
  let verts: V3[] = [
    [-1, t, 0], [1, t, 0], [-1, -t, 0], [1, -t, 0],
    [0, -1, t], [0, 1, t], [0, -1, -t], [0, 1, -t],
    [t, 0, -1], [t, 0, 1], [-t, 0, -1], [-t, 0, 1],
  ].map((v) => scale(norm(v as V3), 0.5));
  let faces = [
    [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
    [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
    [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
    [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
  ];
  const mids = new Map<string, number>();
  const mid = (a: number, b: number) => {
    const k = a < b ? `${a},${b}` : `${b},${a}`;
    let i = mids.get(k);
    if (i === undefined) {
      i = verts.length;
      verts.push(scale(norm(add(verts[a], verts[b])), 0.5));
      mids.set(k, i);
    }
    return i;
  };
  const next: number[][] = [];
  for (const [a, b, c] of faces) {
    const ab = mid(a, b), bc = mid(b, c), ca = mid(c, a);
    next.push([a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]);
  }
  faces = next;
  const m = new MeshBuilder();
  for (const [a, b, c] of faces) m.tri(verts[a], verts[b], verts[c], undefined, O);
  return m.build();
}

export function primitiveMeshes(): Float32Array[] {
  return [box(), cylinder(10, 0.5), cylinder(8, 0), wedge(), sphere()];
}
