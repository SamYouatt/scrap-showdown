//! Thin wrapper over rapier's `PhysicsWorld` with the game's collision groups
//! and contact-force events. The fight owns one of these as a resource.
use bevy::prelude::*;
use rapier3d::prelude::*;
use std::sync::mpsc;

pub const GRAVITY: f32 = 20.0;
pub const STEP: f32 = 1.0 / 60.0;

pub mod grp {
    pub const GROUND: u32 = 1;
    pub const ROBOT: u32 = 2;
    pub const WHEEL: u32 = 4;
    pub const DEBRIS: u32 = 8;
    pub const WALL: u32 = 16;
    pub const ALL: u32 = 0xffff;
}

pub fn groups(member: u32, filter: u32) -> InteractionGroups {
    InteractionGroups::new(Group::from_bits_truncate(member), Group::from_bits_truncate(filter), InteractionTestMode::And)
}

pub struct Phys {
    pub w: PhysicsWorld,
    events: ChannelEventCollector,
    // receivers are !Sync; the mutexes let Phys live in a Bevy resource
    force_rx: std::sync::Mutex<mpsc::Receiver<ContactForceEvent>>,
    col_rx: std::sync::Mutex<mpsc::Receiver<CollisionEvent>>,
}

impl Phys {
    pub fn new() -> Self {
        let mut w = PhysicsWorld::new();
        w.gravity = Vec3::new(0.0, -GRAVITY, 0.0);
        w.integration_parameters.dt = STEP;
        w.integration_parameters.num_solver_iterations = 6;
        let (ctx, crx) = mpsc::channel();
        let (ftx, frx) = mpsc::channel();
        let (stx, _) = mpsc::channel();
        Phys { w, events: ChannelEventCollector::new(ctx, ftx, stx), force_rx: frx.into(), col_rx: crx.into() }
    }

    pub fn step(&mut self) -> Vec<ContactForceEvent> {
        self.w.step_with_events(&(), &self.events);
        while self.col_rx.lock().unwrap().try_recv().is_ok() {}
        self.force_rx.lock().unwrap().try_iter().collect()
    }

    /// Fixed (static) collider.
    pub fn fixed(&mut self, c: ColliderBuilder) -> ColliderHandle {
        self.w.insert_collider(c, None)
    }

    pub fn body(&self, h: RigidBodyHandle) -> &RigidBody {
        &self.w.bodies[h]
    }
    pub fn body_mut(&mut self, h: RigidBodyHandle) -> &mut RigidBody {
        &mut self.w.bodies[h]
    }

    /// First hit straight along `dir`, skipping `skip` (a body) and any collider
    /// rejected by `keep`.
    pub fn ray(&self, from: Vec3, dir: Vec3, len: f32, skip: Option<RigidBodyHandle>, keep: &dyn Fn(ColliderHandle) -> bool) -> Option<(ColliderHandle, f32, Vec3)> {
        let pred = |h: ColliderHandle, _: &Collider| keep(h);
        let mut f = QueryFilter::default().predicate(&pred);
        if let Some(b) = skip {
            f = f.exclude_rigid_body(b);
        }
        self.w.cast_ray_and_get_normal(&Ray::new(from, dir), len, true, f).map(|(h, hit)| (h, hit.time_of_impact, hit.normal))
    }

    /// Every collider overlapping `shape` at `pose`.
    pub fn overlaps(&self, pose: Pose, shape: &dyn Shape) -> Vec<ColliderHandle> {
        self.w.intersect_shape(pose, shape, QueryFilter::default()).map(|(h, _)| h).collect()
    }

    pub fn collider(&self, h: ColliderHandle) -> Option<&Collider> {
        self.w.colliders.get(h)
    }

    /// Solver contacts between `c` and anything else: (other, mean point, normal
    /// pointing from c to other, total impulse).
    pub fn contacts_of(&self, c: ColliderHandle) -> Vec<(ColliderHandle, Vec3, Vec3, f32)> {
        let mut out = vec![];
        for pair in self.w.contact_pairs_with(c) {
            let other = if pair.collider1 == c { pair.collider2 } else { pair.collider1 };
            let flip = pair.collider1 != c;
            let mut pt = Vec3::ZERO;
            let mut n = 0usize;
            let mut imp = 0.0;
            let mut nrm = Vec3::ZERO;
            for m in pair.manifolds() {
                for sc in m.data.solver_contacts.iter() {
                    let (a, b) = m.data.solver_contact_world_points(sc, &self.w.bodies);
                    pt += (a + b) * 0.5;
                    n += 1;
                }
                for tc in m.points.iter() {
                    imp += tc.data.impulse;
                }
                nrm = m.data.normal;
            }
            if n > 0 {
                out.push((other, pt / n as f32, if flip { -nrm } else { nrm }, imp));
            }
        }
        out
    }
}
