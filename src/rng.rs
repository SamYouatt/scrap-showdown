//! Tiny global PRNG (xorshift64*). Good enough for gameplay randomness and
//! works the same on native and wasm without pulling in getrandom at runtime.
use std::sync::atomic::{AtomicU64, Ordering};

static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

fn next_u64() -> u64 {
    let mut x = STATE.load(Ordering::Relaxed);
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    STATE.store(x, Ordering::Relaxed);
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Uniform in [0, 1).
pub fn rand() -> f32 {
    (next_u64() >> 40) as f32 / (1u64 << 24) as f32
}

/// Uniform in [-1, 1).
pub fn signed() -> f32 {
    rand() * 2.0 - 1.0
}

pub fn chance(p: f32) -> bool {
    rand() < p
}

pub fn pick<T: Copy>(xs: &[T]) -> T {
    xs[((rand() * xs.len() as f32) as usize).min(xs.len() - 1)]
}
