// Small tuple-based vector/quaternion helpers for gameplay code.
// Matrices for the GPU come from wgpu-matrix (see camera.ts).

export type V3 = [number, number, number];
export type Quat = [number, number, number, number]; // x, y, z, w

export const v3 = (x = 0, y = 0, z = 0): V3 => [x, y, z];
export const copy = (a: V3): V3 => [a[0], a[1], a[2]];
export const add = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (a: V3, s: number): V3 => [a[0] * s, a[1] * s, a[2] * s];
export const addScaled = (a: V3, b: V3, s: number): V3 => [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s];
export const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a: V3, b: V3): V3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
export const len = (a: V3) => Math.hypot(a[0], a[1], a[2]);
export const dist = (a: V3, b: V3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
export const distXZ = (a: V3, b: V3) => Math.hypot(a[0] - b[0], a[2] - b[2]);
export const lerp = (a: V3, b: V3, t: number): V3 => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];
export function norm(a: V3, fallback: V3 = [0, 1, 0]): V3 {
  const l = len(a);
  return l > 1e-9 ? [a[0] / l, a[1] / l, a[2] / l] : copy(fallback);
}

export const clamp = (x: number, lo: number, hi: number) => (x < lo ? lo : x > hi ? hi : x);
export const mix = (a: number, b: number, t: number) => a + (b - a) * t;
export const smoothstep = (t: number) => t * t * (3 - 2 * t);
/** Frame-rate independent exponential approach factor. */
export const damp = (rate: number, dt: number) => 1 - Math.exp(-rate * dt);
export function wrapAngle(a: number) {
  while (a > Math.PI) a -= Math.PI * 2;
  while (a < -Math.PI) a += Math.PI * 2;
  return a;
}

export const qIdentity = (): Quat => [0, 0, 0, 1];
export function qAxisAngle(axis: V3, angle: number): Quat {
  const s = Math.sin(angle / 2);
  return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(angle / 2)];
}
export function qMul(a: Quat, b: Quat): Quat {
  const [ax, ay, az, aw] = a;
  const [bx, by, bz, bw] = b;
  return [
    aw * bx + ax * bw + ay * bz - az * by,
    aw * by - ax * bz + ay * bw + az * bx,
    aw * bz + ax * by - ay * bx + az * bw,
    aw * bw - ax * bx - ay * by - az * bz,
  ];
}
export function qRotate(q: Quat, v: V3): V3 {
  const u: V3 = [q[0], q[1], q[2]];
  const t = scale(cross(u, v), 2);
  return add(add(v, scale(t, q[3])), cross(u, t));
}
export function qNlerp(a: Quat, b: Quat, t: number): Quat {
  const d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
  const s = d < 0 ? -1 : 1;
  const r: Quat = [
    a[0] + (b[0] * s - a[0]) * t,
    a[1] + (b[1] * s - a[1]) * t,
    a[2] + (b[2] * s - a[2]) * t,
    a[3] + (b[3] * s - a[3]) * t,
  ];
  const l = Math.hypot(r[0], r[1], r[2], r[3]);
  return [r[0] / l, r[1] / l, r[2] / l, r[3] / l];
}
/** Quaternion from an orthonormal basis given as the images of the X, Y and Z axes. */
export function qFromBasis(x: V3, y: V3, z: V3): Quat {
  const m00 = x[0], m10 = x[1], m20 = x[2];
  const m01 = y[0], m11 = y[1], m21 = y[2];
  const m02 = z[0], m12 = z[1], m22 = z[2];
  const tr = m00 + m11 + m22;
  if (tr > 0) {
    const s = Math.sqrt(tr + 1) * 2;
    return [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s];
  } else if (m00 > m11 && m00 > m22) {
    const s = Math.sqrt(1 + m00 - m11 - m22) * 2;
    return [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s];
  } else if (m11 > m22) {
    const s = Math.sqrt(1 + m11 - m00 - m22) * 2;
    return [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s];
  }
  const s = Math.sqrt(1 + m22 - m00 - m11) * 2;
  return [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s];
}
/** Rotation whose local +Z faces `fwd` and local +Y leans towards `up`. */
export function qLook(fwd: V3, up: V3 = [0, 1, 0]): Quat {
  const z = norm(fwd, [0, 0, 1]);
  let x = cross(up, z);
  if (len(x) < 1e-5) x = cross([1, 0, 0], z);
  x = norm(x);
  const y = cross(z, x);
  return qFromBasis(x, y, z);
}
/** Rotation whose local +Y runs along `dir` and local +X follows `side` as closely as possible. */
export function qAlongY(dir: V3, side: V3): Quat {
  const y = norm(dir);
  let z = cross(side, y);
  if (len(z) < 1e-5) z = cross([0, 0, 1], y);
  if (len(z) < 1e-5) z = cross([1, 0, 0], y);
  z = norm(z);
  const x = cross(y, z);
  return qFromBasis(x, y, z);
}

/** A rigid transform: position plus rotation. */
export interface Frame {
  p: V3;
  q: Quat;
}
export const toWorld = (f: Frame, local: V3): V3 => add(f.p, qRotate(f.q, local));

export function rng(seed: number) {
  let a = seed >>> 0 || 1;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return {
    next,
    range: (lo: number, hi: number) => lo + (hi - lo) * next(),
    int: (lo: number, hi: number) => Math.floor(lo + (hi - lo + 1) * next()),
    pick: <T>(arr: readonly T[]): T => arr[Math.floor(next() * arr.length)],
    chance: (p: number) => next() < p,
  };
}
export type Rng = ReturnType<typeof rng>;

function hash2(x: number, y: number) {
  let h = Math.imul(x | 0, 374761393) + Math.imul(y | 0, 668265263);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}
export function valueNoise(x: number, y: number) {
  const xi = Math.floor(x), yi = Math.floor(y);
  const xf = smoothstep(x - xi), yf = smoothstep(y - yi);
  const a = hash2(xi, yi), b = hash2(xi + 1, yi);
  const c = hash2(xi, yi + 1), d = hash2(xi + 1, yi + 1);
  return mix(mix(a, b, xf), mix(c, d, xf), yf);
}
export function fbm(x: number, y: number, octaves = 4) {
  let sum = 0, amp = 0.5, f = 1;
  for (let i = 0; i < octaves; i++) {
    sum += amp * valueNoise(x * f + i * 17.3, y * f - i * 9.1);
    amp *= 0.5;
    f *= 2.03;
  }
  return sum;
}

export function hex(s: string): V3 {
  const n = parseInt(s.replace('#', ''), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}
