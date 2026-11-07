// Inverse kinematics for limb chains.
import { V3, addScaled, copy, dist, lerp, norm, sub } from './math';

/**
 * FABRIK: alternately drag the chain from the target back to the root and from
 * the root out to the target, keeping every segment length fixed. The starting
 * pose decides which way the joints bend, so callers seed it with bentChain().
 */
export function fabrik(chain: V3[], lengths: number[], target: V3, iterations = 12) {
  const n = chain.length;
  const root = copy(chain[0]);
  const total = lengths.reduce((a, b) => a + b, 0);
  if (dist(root, target) >= total) {
    const d = norm(sub(target, root));
    for (let i = 1; i < n; i++) chain[i] = addScaled(chain[i - 1], d, lengths[i - 1]);
    return;
  }
  for (let it = 0; it < iterations; it++) {
    chain[n - 1] = copy(target);
    for (let i = n - 2; i >= 0; i--) chain[i] = addScaled(chain[i + 1], norm(sub(chain[i], chain[i + 1])), lengths[i]);
    chain[0] = copy(root);
    for (let i = 1; i < n; i++) chain[i] = addScaled(chain[i - 1], norm(sub(chain[i], chain[i - 1])), lengths[i - 1]);
    if (dist(chain[n - 1], target) < 1e-3) break;
  }
}

/** Joints spaced along a quadratic curve from root to tip that bulges towards `pole`. */
export function bentChain(root: V3, pole: V3, tip: V3, joints: number): V3[] {
  const out: V3[] = [];
  for (let i = 0; i < joints; i++) {
    const t = i / (joints - 1);
    out.push(lerp(lerp(root, pole, t), lerp(pole, tip, t), t));
  }
  return out;
}
