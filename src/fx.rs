//! Particles (sparks, smoke, dust, bolts, paint shards) + camera shake.
//! Each pool renders as ONE mesh rebuilt on the CPU every frame (instancing by
//! hand), so the whole effect layer is five draw calls.
//!
//! A screen that wants effects inserts an [`Fx`] resource on enter and
//! removes it on exit; while it exists [`plugin`] simulates and redraws it.
use crate::models::{self, box_polys, cyl_polys, ico_polys, paint, Col, Geo};
use crate::rng;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, update_fx.run_if(resource_exists::<Fx>));
}

/// Real time, so effects keep moving (slowly) through hit-stop; the owning
/// screen sets `time_scale` for slow motion.
fn update_fx(mut fx: ResMut<Fx>, time: Res<Time<Real>>, mut meshes: ResMut<Assets<Mesh>>) {
    let dt = time.delta_secs().min(0.05) * fx.time_scale;
    fx.update(dt);
    fx.write(&mut meshes);
}

struct PConf {
    max: usize,
    gravity: f32,
    drag: f32,
    /// <0 = die on floor
    bounce: f32,
    stretch: bool,
    /// size multiplier over life (smoke)
    grow: f32,
    spin: bool,
    /// floor applies (sparks, bits, shards); smoke and dust float
    floored: bool,
}

#[derive(Clone, Copy, Default)]
struct P {
    pos: Vec3,
    vel: Vec3,
    rot: Vec3,
    life: f32,
    max: f32,
    size: f32,
    c1: Col,
    c2: Col,
}

struct Pool {
    c: PConf,
    base: Geo,
    ps: Vec<P>,
    cursor: usize,
    mesh: Handle<Mesh>,
}

impl Pool {
    fn emit(&mut self, pos: Vec3, vel: Vec3, life: f32, size: f32, c1: Col, c2: Option<Col>) {
        let i = self.cursor;
        self.cursor = (self.cursor + 1) % self.c.max;
        self.ps[i] = P { pos, vel, rot: Vec3::new(rng::rand() * 6.0, rng::rand() * 6.0, rng::rand() * 6.0), life, max: life, size, c1, c2: c2.unwrap_or(c1) };
    }

    fn update(&mut self, dt: f32, floor: fn(f32, f32) -> f32) {
        let c = &self.c;
        for p in self.ps.iter_mut() {
            if p.life <= 0.0 {
                continue;
            }
            p.life -= dt;
            if p.life <= 0.0 {
                continue;
            }
            p.vel.y -= c.gravity * dt;
            p.vel *= (-c.drag * dt).exp();
            p.pos += p.vel * dt;
            let fy = if c.floored { floor(p.pos.x, p.pos.z) } else { -100.0 };
            if p.pos.y < fy {
                if c.bounce < 0.0 {
                    p.life = 0.0;
                    continue;
                }
                p.pos.y = fy;
                p.vel.y = p.vel.y.abs() * c.bounce;
                p.vel.x *= 0.6;
                p.vel.z *= 0.6;
                if c.spin {
                    p.rot.x *= 0.5;
                }
            }
            if c.spin {
                // tumble in the air; once landed, settle flat so litter reads as plates
                if p.pos.y > fy + 0.03 {
                    p.rot.x += dt * 9.0;
                    p.rot.y += dt * 7.0;
                } else {
                    let k = (-dt * 10.0).exp();
                    p.rot.x *= k;
                    p.rot.z *= k;
                }
            }
        }
    }

    /// Write the live particles into the pool's mesh.
    fn write(&self, meshes: &mut Assets<Mesh>) {
        let c = &self.c;
        let n = self.base.pos.len();
        let live = self.ps.iter().filter(|p| p.life > 0.0).count();
        let mut pos = Vec::with_capacity(live * n);
        let mut nrm = Vec::with_capacity(live * n);
        let mut col = Vec::with_capacity(live * n);
        for p in self.ps.iter().filter(|p| p.life > 0.0) {
            let t = 1.0 - p.life / p.max;
            let mut sz = p.size;
            if c.grow > 0.0 {
                sz *= (1.0 + t * c.grow) * if t > 0.6 { 1.0 - (t - 0.6) / 0.4 } else { 1.0 };
            } else {
                sz *= (p.life / p.max * 3.0).min(1.0);
            }
            let (rot, scale) = if c.stretch {
                let sp = p.vel.length();
                (Quat::from_rotation_arc(Vec3::Z, p.vel / (sp + 1e-6)), Vec3::new(sz, sz, sz + sp * 0.035))
            } else {
                (Quat::from_euler(EulerRot::XYZ, p.rot.x, p.rot.y, p.rot.z), Vec3::splat(sz))
            };
            let cc = p.c1.lerp(p.c2, t);
            for i in 0..n {
                pos.push((rot * (self.base.pos[i] * scale) + p.pos).to_array());
                nrm.push((rot * self.base.nrm[i]).to_array());
                let b = self.base.col[i] * cc;
                col.push([b.x, b.y, b.z, 1.0]);
            }
        }
        if let Some(mut m) = meshes.get_mut(&self.mesh) {
            if pos.is_empty() {
                // a degenerate triangle: empty meshes are not drawable everywhere
                pos = vec![[0.0, -100.0, 0.0]; 3];
                nrm = vec![[0.0, 1.0, 0.0]; 3];
                col = vec![[0.0; 4]; 3];
            }
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        }
    }
}

fn spark1() -> Col {
    Vec3::new(4.0, 3.2, 1.4)
}
fn spark2() -> Col {
    Vec3::new(1.6, 0.35, 0.05)
}

#[derive(Resource)]
pub struct Fx {
    sparks: Pool,
    smoke: Pool,
    dust: Pool,
    bits: Pool,
    shards: Pool,
    shake: f32,
    /// multiplies real time when simulating
    pub time_scale: f32,
    /// floor height at (x, z) for particles that land
    floor: fn(f32, f32) -> f32,
}

impl Fx {
    /// Create the pools and spawn their (empty) meshes with `scope`.
    pub fn new(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, floor: fn(f32, f32) -> f32, scope: impl Bundle + Clone) -> Fx {
        let mut mk = |base: Geo, mat: StandardMaterial, c: PConf, shadows: bool| -> Pool {
            let mesh = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, -100.0, 0.0]; 3])
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3])
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]));
            let mut e = commands.spawn((Name::new("Particles"), Mesh3d(mesh.clone()), MeshMaterial3d(mats.add(mat)), Transform::IDENTITY, bevy::camera::visibility::NoFrustumCulling, scope.clone()));
            if !shadows {
                e.insert(bevy::light::NotShadowCaster);
            }
            let max = c.max;
            Pool { c, base, ps: vec![P::default(); max], cursor: 0, mesh }
        };
        let white = Vec3::ONE;
        let sparks = mk(
            paint(&box_polys(1.0, 1.0, 1.0), white, 0.0, 0.0),
            StandardMaterial { unlit: true, ..default() },
            PConf { max: 600, gravity: 14.0, drag: 1.5, bounce: 0.35, stretch: true, grow: 0.0, spin: false, floored: true },
            false,
        );
        let smoke = mk(
            paint(&ico_polys(1.0), white, 0.0, 0.0),
            StandardMaterial { perceptual_roughness: 1.0, ..default() },
            PConf { max: 300, gravity: -1.2, drag: 2.2, bounce: 0.0, stretch: false, grow: 2.2, spin: true, floored: false },
            true,
        );
        // soft, see-through puffs (opaque ones read as boulders)
        let dust = mk(
            paint(&ico_polys(1.0), white, 0.0, 0.0),
            StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.5), alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() },
            PConf { max: 300, gravity: -0.3, drag: 3.0, bounce: 0.0, stretch: false, grow: 1.2, spin: true, floored: false },
            false,
        );
        let bits = mk(
            paint(&cyl_polys(0.5, 0.5, 0.6, 6), white, 0.0, 0.0),
            StandardMaterial { perceptual_roughness: 0.5, metallic: 0.4, ..default() },
            PConf { max: 400, gravity: 20.0, drag: 0.3, bounce: 0.4, stretch: false, grow: 0.0, spin: true, floored: true },
            true,
        );
        // flakes of paint and plate torn off on hits: flat, tumbling, they litter the floor
        let shards = mk(
            paint(&box_polys(1.0, 0.18, 0.7), white, 0.0, 0.0),
            StandardMaterial { perceptual_roughness: 0.7, metallic: 0.2, ..default() },
            PConf { max: 500, gravity: 18.0, drag: 0.6, bounce: 0.25, stretch: false, grow: 0.0, spin: true, floored: true },
            true,
        );
        Fx { sparks, smoke, dust, bits, shards, shake: 0.0, time_scale: 1.0, floor }
    }

    /// Torn paint/plate flakes in the victim's colours, flung along `dir`.
    pub fn shard_burst(&mut self, p: Vec3, n: usize, paint_c: Col, dir: Option<Vec3>, speed: f32) {
        for _ in 0..n {
            let mut v = Vec3::new(rng::rand() - 0.5, 0.5 + rng::rand() * 0.9, rng::rand() - 0.5).normalize();
            if let Some(d) = dir {
                v += d * 1.2;
            }
            v = v.normalize_or(Vec3::Y) * speed * (0.4 + rng::rand());
            let r = rng::rand();
            // mostly paint, some bare steel, a little rust
            let c = if r < 0.55 { paint_c * (0.8 + rng::rand() * 0.3) } else if r < 0.85 { Vec3::new(0.55, 0.56, 0.6) } else { Vec3::new(0.6, 0.3, 0.15) };
            self.shards.emit(p, v, 4.0 + rng::rand() * 3.0, 0.1 + rng::rand() * 0.14, c, None);
        }
    }

    pub fn spark_burst(&mut self, p: Vec3, n: usize, dir: Option<Vec3>, speed: f32, spread: f32) {
        for _ in 0..n {
            let mut v = Vec3::new(rng::rand() - 0.5, rng::rand() * 0.8, rng::rand() - 0.5).normalize_or(Vec3::Y) * spread;
            if let Some(d) = dir {
                v += d;
            }
            v = v.normalize_or(Vec3::Y) * speed * (0.4 + rng::rand());
            self.sparks.emit(p, v, 0.25 + rng::rand() * 0.45, 0.025 + rng::rand() * 0.02, spark1(), Some(spark2()));
        }
    }

    pub fn smoke_puff(&mut self, p: Vec3, n: usize, dark: f32, size: f32) {
        for _ in 0..n {
            let v = Vec3::new((rng::rand() - 0.5) * 0.8, 0.6 + rng::rand() * 0.6, (rng::rand() - 0.5) * 0.8);
            let g = dark + rng::rand() * 0.1;
            self.smoke.emit(p, v, 0.8 + rng::rand() * 0.8, size * (0.7 + rng::rand() * 0.6), Vec3::new(g, g, g * 1.05), Some(Vec3::splat(g + 0.25)));
        }
    }

    pub fn fire(&mut self, p: Vec3) {
        let v = Vec3::new((rng::rand() - 0.5) * 0.4, 1.0 + rng::rand(), (rng::rand() - 0.5) * 0.4);
        self.smoke.emit(p, v, 0.4 + rng::rand() * 0.2, 0.08 + rng::rand() * 0.05, Vec3::new(3.0, 1.4, 0.2), Some(Vec3::new(0.3, 0.28, 0.28)));
    }

    pub fn dust_puff(&mut self, p: Vec3, n: usize, color: Option<Col>, size: f32, speed: f32) {
        let color = color.unwrap_or_else(|| models::hexc("#d8b98a"));
        for _ in 0..n {
            let a = rng::rand() * std::f32::consts::TAU;
            let v = Vec3::new(a.cos() * speed * (0.5 + rng::rand()), 0.3 + rng::rand() * 0.8, a.sin() * speed * (0.5 + rng::rand()));
            self.dust.emit(p, v, 0.5 + rng::rand() * 0.6, size * (0.6 + rng::rand() * 0.8), color, Some(color * 1.15));
        }
    }
    pub fn dust(&mut self, p: Vec3, n: usize) {
        self.dust_puff(p, n, None, 0.18, 2.0);
    }

    pub fn bolt_burst(&mut self, p: Vec3, n: usize, dir: Option<Vec3>) {
        let cols = [models::hexc("#9a9ca3"), models::hexc("#5a5c64"), models::hexc("#c87a40")];
        for _ in 0..n {
            let mut v = Vec3::new(rng::rand() - 0.5, 0.6 + rng::rand(), rng::rand() - 0.5) * 4.0;
            if let Some(d) = dir {
                v += d * 3.0;
            }
            self.bits.emit(p, v, 3.5 + rng::rand() * 3.0, 0.05 + rng::rand() * 0.05, rng::pick(&cols), None);
        }
    }

    fn update(&mut self, dt: f32) {
        let floor = self.floor;
        for p in [&mut self.sparks, &mut self.smoke, &mut self.dust, &mut self.bits, &mut self.shards] {
            p.update(dt, floor);
        }
        self.shake = (self.shake - dt * 2.2).max(0.0);
    }

    fn write(&self, meshes: &mut Assets<Mesh>) {
        for p in [&self.sparks, &self.smoke, &self.dust, &self.bits, &self.shards] {
            p.write(meshes);
        }
    }

    pub fn add_shake(&mut self, a: f32) {
        self.shake = (self.shake + a).min(1.2);
    }

    pub fn shake_offset(&self, t: f32) -> Vec3 {
        let s = self.shake * self.shake * 0.35;
        Vec3::new((t * 61.0).sin() * s, (t * 53.0 + 1.0).sin() * s, (t * 47.0 + 2.0).sin() * s)
    }
}
