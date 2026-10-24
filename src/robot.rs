//! Physics robot: one rigid body made of per-part colliders placed anywhere on
//! the frame, raycast suspension for wheels, IK walking for legs, weapons that
//! act along their own mounted axes, and parts that break off into debris.
use crate::audio::{Hum, HumKind, Sfx, Snd};
use crate::fx::Fx;
use crate::models::{self, Col, Mats};
use crate::partmodels::{build_part, collision_shapes, hammer_arm_hulls, ColShape};
use crate::parts::*;
use crate::physics::{grp, groups, Phys, GRAVITY};
use crate::rng;
use bevy::prelude::*;
use rapier3d::prelude::*;
use std::collections::HashMap;
use std::f32::consts::PI;

#[derive(Clone, Copy, Default, Debug)]
pub struct Input {
    pub throttle: f32,
    /// +1 = turn right
    pub turn: f32,
    pub fire: bool,
    pub boost: bool,
    /// handbrake: rear wheels lock and slide
    pub brake: bool,
    /// per weapon slot (player); without it `fire` triggers everything
    pub fire_slots: Option<[bool; 2]>,
}

pub struct Boost;
impl Boost {
    pub const TIME: f32 = 0.45;
    pub const COOLDOWN: f32 = 3.0;
}

/// (team, part index) for every robot collider.
pub type PartOf = HashMap<ColliderHandle, (usize, usize)>;

/// Things that happen during a robot's step that the battle resolves after.
pub enum Ev {
    Hit { att: Option<usize>, tgt: (usize, usize), dmg: f32, pt: Vec3, big: bool, word: Option<&'static str> },
    HitStop(f32),
}

pub struct Ctx<'a> {
    pub phys: &'a mut Phys,
    pub fx: &'a mut Fx,
    pub sfx: &'a mut Vec<Sfx>,
    pub part_of: &'a PartOf,
    pub ev: &'a mut Vec<Ev>,
    pub time: f32,
}

/// Eye looks: calm, angry after a hit, dead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eyes {
    Calm,
    Angry,
    Dead,
}

/// Glowing eye materials shared by every robot.
#[derive(Resource)]
pub struct EyeMats([Handle<StandardMaterial>; 3]);

impl EyeMats {
    pub fn get(&self, e: Eyes) -> Handle<StandardMaterial> {
        self.0[e as usize].clone()
    }
}

impl FromWorld for EyeMats {
    fn from_world(world: &mut World) -> Self {
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        EyeMats([
            models::glow_mat(&mut mats, models::hexc("#ffd25a"), 2.4),
            models::glow_mat(&mut mats, models::hexc("#ff6a3c"), 2.4),
            models::glow_mat(&mut mats, models::hexc("#3a3335"), 1.0),
        ])
    }
}

pub struct LegEnts {
    pub upper: Entity,
    pub lower: Entity,
    pub foot: Entity,
}

pub struct RPart {
    pub placed: Placed,
    pub kind: PartKind,
    pub hp: f32,
    pub max_hp: f32,
    /// bot-local
    pub pos: Vec3,
    pub quat: Quat,
    pub colliders: Vec<ColliderHandle>,
    /// part-frame pose of each static collider
    pub col_pose: Vec<(Vec3, Quat)>,
    pub attached: bool,
    // drive
    /// bot-local ray origin (wheel centre / hip)
    pub mount: Vec3,
    pub axle: Vec3,
    pub works: bool,
    pub dist: f32,
    pub grounded: bool,
    pub wheel_angle: f32,
    /// car steering: +1 front wheel, -0.5 rear (counter-steer), 0 fixed
    pub steer_k: f32,
    pub steer: f32,
    /// +1 = driver's left of the COM, -1 = right
    pub side: f32,
    pub foot: Option<Vec3>,
    pub foot_from: Vec3,
    pub foot_to: Vec3,
    pub step_t: f32,
    pub planted: bool,
    pub gait: usize,
    // weapons
    pub spin: f32,
    pub angle: f32,
    pub cooldown: f32,
    pub anim: f32,
    pub struck: bool,
    pub stop_at: Option<f32>,
    pub slot: usize,
    // fx
    pub punch: f32,
    pub smoke_t: f32,
    pub loosen: Quat,
    pub scorched: bool,
    // visuals
    pub ent: Entity,
    pub spin_ent: Option<(Entity, Vec3)>,
    pub arm_ent: Option<(Entity, Vec3)>,
    pub eyes: Vec<(Entity, Transform)>,
    pub whip: Option<(Entity, Vec3, Vec3)>,
    pub legs: Option<LegEnts>,
}

impl RPart {
    pub fn def(&self) -> &'static PartDef {
        self.kind.def()
    }
}

/// Hammer head position in part-local space for a given swing angle.
pub fn hammer_head(angle: f32) -> Vec3 {
    HAMMER.pivot + Quat::from_rotation_x(-angle) * Vec3::new(0.0, 0.0, HAMMER.len)
}

/// Where a weapon actually hits, part-local: used by the AI to aim.
pub fn strike_point(kind: PartKind) -> Option<Vec3> {
    Some(match kind {
        PartKind::Spinner => Vec3::new(0.0, 0.0, 0.2),
        PartKind::Hammer => hammer_head(HAMMER.strike),
        PartKind::Flipper => Vec3::new(0.0, FLIP.hinge_y + FLIP.len * 0.7, 0.05),
        PartKind::Spike => Vec3::new(0.0, 0.0, 0.72),
        PartKind::Wedge => Vec3::new(0.0, -0.15, 0.6),
        _ => return None,
    })
}

/// Rotation whose +Z points along `dir` (three.js Object3D.lookAt).
fn look_z(dir: Vec3, up: Vec3) -> Quat {
    let d = dir.normalize_or(Vec3::Z);
    let up = if d.cross(up).length_squared() < 1e-6 { Vec3::X } else { up };
    Transform::IDENTITY.looking_to(-d, up).rotation
}

const Y_TO_Z: Quat = Quat::from_xyzw(0.70710677, 0.0, 0.0, 0.70710677); // rapier cylinders are along Y; ours along Z

pub struct Robot {
    pub team: usize,
    pub body: RigidBodyHandle,
    pub root: Entity,
    pub parts: Vec<RPart>,
    pub core: usize,
    pub paint: Col,
    pub input: Input,
    pub power_ratio: f32,
    pub alive: bool,
    /// match over: weapons stop
    pub halted: bool,
    pub q: Quat,
    pub pos: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
    /// bot-local
    pub com: Vec3,
    pub mass: f32,
    pub grounded_frac: f32,
    pub top_speed: f32,
    pub agility: f32,
    pub boost_t: f32,
    pub boost_cd: f32,
    pub car_steer: bool,
    pub invert_time: f32,
    pub side_time: f32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
    pub motor: Hum,
    pub spin_hum: Option<Hum>,
    pub blink: f32,
    /// >0 angry eyes after taking a hit
    pub mood: f32,
    pub label: String,
    last_gait: usize,
    gait_t: f32,
    gait_wants: [bool; 2],
}

/// Everything needed to spawn a robot's visuals and colliders.
pub struct Spawn<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub shared: &'a Mats,
    pub glow: &'a EyeMats,
    pub sfx: &'a mut Vec<Sfx>,
}

impl Robot {
    /// `ground` is where the bot's lowest contact point should rest.
    pub fn new<S: Bundle + Clone>(sp: &mut Spawn, phys: &mut Phys, part_of: &mut PartOf, design: &Design, ground: Vec3, yaw: f32, team: usize, scope: S) -> Robot {
        let stats = compute_stats(design);
        let rot = Quat::from_rotation_y(yaw);
        let body = phys.w.insert_body(
            RigidBodyBuilder::dynamic()
                .pose(Pose::from_parts(Vec3::new(ground.x, ground.y - stats.lowest + 0.06, ground.z), rot))
                .linear_damping(0.05)
                .angular_damping(0.6)
                .ccd_enabled(true),
        );
        let root = sp.commands.spawn((Name::new(design.name.clone()), Transform::from_rotation(rot), Visibility::default(), scope.clone())).id();
        let paint = models::col_of(design.paint);
        let mut r = Robot {
            team,
            body,
            root,
            parts: vec![],
            core: 0,
            paint,
            input: Input::default(),
            power_ratio: stats.power_ratio,
            alive: true,
            halted: false,
            q: rot,
            pos: Vec3::ZERO,
            up: Vec3::Y,
            fwd: Vec3::Z,
            com: Vec3::ZERO,
            mass: 1.0,
            grounded_frac: 0.0,
            top_speed: 7.2,
            agility: 1.0,
            boost_t: 0.0,
            boost_cd: 0.0,
            car_steer: false,
            invert_time: 0.0,
            side_time: 0.0,
            damage_dealt: 0.0,
            damage_taken: 0.0,
            motor: Hum { owner: root, kind: HumKind::Motor },
            spin_hum: None,
            blink: 0.0,
            mood: 0.0,
            label: design.name.clone(),
            last_gait: 1,
            gait_t: -1.0,
            gait_wants: [false; 2],
        };
        let mut leg_idx = 0;
        for pl in &design.parts {
            r.add_part(sp, phys, part_of, pl, leg_idx, scope.clone());
            if pl.kind == PartKind::Leg {
                leg_idx += 1;
            }
        }
        r.core = r.parts.iter().position(|p| p.kind == PartKind::Core).unwrap_or(0);
        // weapons you trigger: the first one built is weapon 1, the rest weapon 2
        for (i, p) in r.parts.iter_mut().filter(|p| matches!(p.kind, PartKind::Hammer | PartKind::Flipper)).enumerate() {
            p.slot = i.min(1);
        }
        let b = &phys.w.bodies[body];
        r.mass = b.mass();
        r.com = b.local_center_of_mass();
        let com = r.com;
        for p in r.parts.iter_mut() {
            let lat = p.mount.x - com.x;
            if lat.abs() > 0.05 {
                p.side = lat.signum();
            }
        }
        // Legs step in alternating groups by where they sit around the body:
        // diagonal pairs on a quadruped, tripods on a hexapod, left/right on a biped
        let mut legs: Vec<(usize, f32)> = r.parts.iter().enumerate().filter(|(_, p)| p.kind == PartKind::Leg).map(|(i, p)| (i, (p.mount.x - com.x).atan2(p.mount.z - com.z))).collect();
        legs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        for (k, (i, _)) in legs.iter().enumerate() {
            r.parts[*i].gait = k % 2;
        }
        // Wheels ahead of the COM steer, wheels behind counter-steer a little.
        let dz = |p: &RPart| p.mount.z - com.z;
        let wheels: Vec<usize> = (0..r.parts.len()).filter(|&i| r.parts[i].kind.is_wheel() && r.parts[i].works).collect();
        r.car_steer = wheels.iter().any(|&i| dz(&r.parts[i]) > 0.12) && wheels.iter().any(|&i| dz(&r.parts[i]) < -0.12);
        if r.car_steer {
            for &i in &wheels {
                let d = dz(&r.parts[i]);
                r.parts[i].steer_k = if d > 0.12 { 1.0 } else if d < -0.12 { -0.5 } else { 0.0 };
            }
        }
        if r.parts.iter().any(|p| p.kind == PartKind::Spinner) {
            r.spin_hum = Some(Hum { owner: root, kind: HumKind::Spinner });
        }
        sp.sfx.push(r.motor.start());
        if let Some(h) = r.spin_hum {
            sp.sfx.push(h.start());
        }
        let wheel_f: f32 = r.parts.iter().map(|p| match p.kind {
            PartKind::Leg => 230.0,
            PartKind::BigWheel => 200.0,
            PartKind::Wheel => 165.0,
            _ => 0.0,
        }).sum();
        let accel = wheel_f * r.power_ratio / r.mass.max(1.0);
        let inertia = phys.w.bodies[body].mass_properties().local_mprops.principal_inertia();
        let k2 = inertia.y / r.mass.max(1.0);
        r.top_speed = (7.2 * (accel / 30.0).sqrt()).clamp(3.8, 8.5);
        r.agility = (0.3 / k2.max(0.05)).sqrt().clamp(0.6, 1.2) * (accel / 30.0).sqrt().clamp(0.7, 1.1);
        r.read_pose(phys);
        r
    }

    fn add_part<S: Bundle + Clone>(&mut self, sp: &mut Spawn, phys: &mut Phys, part_of: &mut PartOf, pl: &Placed, leg_idx: usize, scope: S) {
        let kind = pl.kind;
        let def = kind.def();
        let model = build_part(kind, self.paint);
        let (pos, quat) = (pl.pos, pl.rot);
        let mut mount = pos;
        let mut axle = quat * Vec3::Z;
        let mut works = true;
        if kind.is_wheel() {
            let (c, a) = wheel_frame(pl);
            mount = c;
            axle = a;
            works = a.y.abs() < 0.6;
        } else if kind == PartKind::Leg {
            mount = quat * Vec3::new(0.0, 0.0, 0.19) + pos;
        }
        let vc = sp.shared.vc.clone();
        let ent = sp.commands.spawn((Mesh3d(sp.meshes.add(model.body.to_mesh())), MeshMaterial3d(vc.clone()), Transform { translation: pos, rotation: quat, ..default() }, scope.clone())).id();
        sp.commands.entity(self.root).add_child(ent);
        let mut spin_ent = None;
        if let Some((g, o)) = &model.spin {
            let e = sp.commands.spawn((Mesh3d(sp.meshes.add(g.to_mesh())), MeshMaterial3d(vc.clone()), Transform::from_translation(*o), scope.clone())).id();
            sp.commands.entity(ent).add_child(e);
            spin_ent = Some((e, *o));
        }
        let mut arm_ent = None;
        if let Some((g, o)) = &model.arm {
            let e = sp.commands.spawn((Mesh3d(sp.meshes.add(g.to_mesh())), MeshMaterial3d(vc.clone()), Transform::from_translation(*o), scope.clone())).id();
            sp.commands.entity(ent).add_child(e);
            arm_ent = Some((e, *o));
        }
        let mut eyes = vec![];
        for (g, t, _) in &model.glow {
            let e = sp.commands.spawn((Mesh3d(sp.meshes.add(g.to_mesh())), MeshMaterial3d(sp.glow.get(Eyes::Calm)), *t, bevy::light::NotShadowCaster, scope.clone())).id();
            sp.commands.entity(ent).add_child(e);
            eyes.push((e, *t));
        }
        let mut whip = None;
        if let Some(w) = &model.whip {
            let mut g = w.rod.clone();
            g.add(&models::ico(0.035, w.tip_col * 2.0), Transform::from_translation(w.tip));
            let e = sp.commands.spawn((Mesh3d(sp.meshes.add(g.to_mesh())), MeshMaterial3d(vc.clone()), Transform::from_translation(w.origin), scope.clone())).id();
            sp.commands.entity(ent).add_child(e);
            whip = Some((e, w.origin, w.tip + Vec3::Y * 0.04));
        }
        let mut legs = None;
        if let Some(l) = &model.leg {
            let mk = |sp: &mut Spawn, g: &models::Geo| sp.commands.spawn((Mesh3d(sp.meshes.add(g.to_mesh())), MeshMaterial3d(vc.clone()), Transform::default(), scope.clone())).id();
            legs = Some(LegEnts { upper: mk(sp, &l.upper), lower: mk(sp, &l.lower), foot: mk(sp, &l.foot) });
        }
        let mut p = RPart {
            placed: pl.clone(),
            kind,
            hp: def.hp * (1.0 - pl.dmg),
            max_hp: def.hp,
            pos,
            quat,
            colliders: vec![],
            col_pose: vec![],
            attached: true,
            mount,
            axle,
            works,
            dist: 0.4,
            grounded: false,
            wheel_angle: 0.0,
            steer_k: 0.0,
            steer: 0.0,
            side: if leg_idx % 2 == 1 { 1.0 } else { -1.0 },
            foot: None,
            foot_from: Vec3::ZERO,
            foot_to: Vec3::ZERO,
            step_t: 1.0,
            planted: false,
            gait: leg_idx % 2,
            spin: 0.0,
            angle: match kind {
                PartKind::Flipper => FLIP.rest,
                PartKind::Hammer => HAMMER.rest,
                _ => 0.0,
            },
            cooldown: 0.0,
            anim: -1.0,
            struck: false,
            stop_at: None,
            slot: 0,
            punch: 0.0,
            smoke_t: rng::rand(),
            loosen: Quat::IDENTITY,
            scorched: false,
            ent,
            spin_ent,
            arm_ent,
            eyes,
            whip,
            legs,
        };
        self.build_colliders(phys, part_of, &mut p);
        self.parts.push(p);
        let i = self.parts.len() - 1;
        for c in self.parts[i].colliders.clone() {
            part_of.insert(c, (self.team, i));
        }
        self.refresh_damage_look(phys, i, sp.commands, sp.shared);
    }

    fn collider_pose(p: &RPart, t: Vec3, q: Quat) -> Pose {
        let pq = p.quat * p.loosen;
        Pose::from_parts(pq * t + p.pos, pq * q)
    }

    fn build_colliders(&self, phys: &mut Phys, _part_of: &mut PartOf, p: &mut RPart) {
        let kind = p.kind;
        let mut shapes: Vec<(ColliderBuilder, Vec3, Quat)> = vec![];
        for s in collision_shapes(kind) {
            match s {
                ColShape::Hull(pts) => {
                    if let Some(b) = ColliderBuilder::convex_hull(&pts) {
                        shapes.push((b, Vec3::ZERO, Quat::IDENTITY));
                    }
                }
                ColShape::Cyl { hh, r, c } => shapes.push((ColliderBuilder::cylinder(hh, r), c, Y_TO_Z)),
            }
        }
        if kind == PartKind::Flipper {
            // the plate, resting as a ramp
            let c = Quat::from_rotation_x(-FLIP.rest) * Vec3::new(0.0, FLIP.len / 2.0, 0.02) + Vec3::new(0.0, FLIP.hinge_y, FLIP.hinge_z);
            shapes.push((ColliderBuilder::cuboid(0.25, FLIP.len / 2.0, 0.025), c, Quat::from_rotation_x(-FLIP.rest)));
        }
        let is_wheel = kind.is_wheel();
        let slippery = matches!(kind, PartKind::Wedge | PartKind::Flipper);
        let n = shapes.len().max(1) as f32;
        for (b, t, q) in shapes {
            p.col_pose.push((t, q));
            let pose = Self::collider_pose(p, t, q);
            let b = b
                .position(pose)
                .mass(kind.def().mass / n)
                // multiply-combine: grippy on the floor (1.0), slippery on another bot.
                // tyre grip comes from the drive model, so a scraping tyre never brakes
                .friction(if is_wheel { 0.0 } else if slippery { 0.08 } else { 0.45 })
                .friction_combine_rule(CoefficientCombineRule::Multiply)
                .restitution(0.15)
                .collision_groups(if is_wheel { groups(grp::WHEEL, grp::GROUND | grp::ROBOT | grp::WHEEL | grp::WALL | grp::DEBRIS) } else { groups(grp::ROBOT, grp::ALL) })
                .active_events(ActiveEvents::CONTACT_FORCE_EVENTS)
                .contact_force_event_threshold(1500.0);
            p.colliders.push(phys.w.insert_collider(b, Some(self.body)));
        }
        if kind == PartKind::Hammer {
            // arm + head: real colliders that follow the swing (see pose_hammer)
            let (shaft, head) = hammer_arm_hulls();
            for pts in [shaft, head] {
                if let Some(b) = ColliderBuilder::convex_hull(&pts) {
                    let b = b
                        .mass(0.3)
                        .friction(0.45)
                        .friction_combine_rule(CoefficientCombineRule::Multiply)
                        .restitution(0.1)
                        .collision_groups(groups(grp::ROBOT, grp::ALL))
                        .active_events(ActiveEvents::CONTACT_FORCE_EVENTS)
                        .contact_force_event_threshold(1500.0);
                    p.colliders.push(phys.w.insert_collider(b, Some(self.body)));
                }
            }
            Self::pose_hammer(phys, p, HAMMER.rest);
        }
    }

    /// Re-seat a part's static colliders after its loosened tilt changes.
    fn place_colliders(phys: &mut Phys, p: &RPart) {
        for (i, (t, q)) in p.col_pose.iter().enumerate() {
            if let Some(c) = p.colliders.get(i).and_then(|h| phys.w.colliders.get_mut(*h)) {
                c.set_position_wrt_parent(Self::collider_pose(p, *t, *q));
            }
        }
    }

    /// Move the hammer's arm/head colliders to a swing angle.
    fn pose_hammer(phys: &mut Phys, p: &RPart, angle: f32) {
        let n = p.col_pose.len();
        let pq = p.quat * p.loosen;
        let pose = Pose::from_parts(pq * HAMMER.pivot + p.pos, pq * Quat::from_rotation_x(-angle));
        for h in p.colliders.iter().skip(n) {
            if let Some(c) = phys.w.colliders.get_mut(*h) {
                c.set_position_wrt_parent(pose);
            }
        }
    }

    // ------------------------------------------------------------------ frames
    pub fn part_world(&self, p: &RPart, local: Vec3) -> Vec3 {
        self.q * (p.quat * local + p.pos) + self.pos
    }
    pub fn dir_world(&self, p: &RPart, local: Vec3) -> Vec3 {
        self.q * (p.quat * local)
    }
    pub fn bot_world(&self, local: Vec3) -> Vec3 {
        self.q * local + self.pos
    }
    pub fn linvel(&self, phys: &Phys) -> Vec3 {
        phys.w.bodies[self.body].linvel()
    }

    pub fn read_pose(&mut self, phys: &Phys) {
        let b = &phys.w.bodies[self.body];
        self.pos = b.translation();
        self.q = *b.rotation();
        self.up = self.q * Vec3::Y;
        self.fwd = self.q * Vec3::Z;
    }

    fn mine(&self, part_of: &PartOf, h: ColliderHandle) -> bool {
        part_of.get(&h).is_some_and(|(t, _)| *t == self.team)
    }

    /// Ground under a point (anything solid that isn't us): (y, collider).
    fn ground_at(&self, ctx: &Ctx, x: f32, y_top: f32, z: f32, len: f32) -> Option<(f32, ColliderHandle)> {
        let po = ctx.part_of;
        let team = self.team;
        ctx.phys
            .ray(Vec3::new(x, y_top, z), Vec3::NEG_Y, len, Some(self.body), &|h| !po.get(&h).is_some_and(|(t, _)| *t == team))
            .map(|(h, toi, _)| (y_top - toi, h))
    }

    fn friction_of(ctx: &Ctx, h: ColliderHandle) -> f32 {
        ctx.phys.collider(h).map(|c| c.friction()).unwrap_or(0.45)
    }

    /// Push the body that owns `h` (if dynamic and not us).
    fn push_other(&self, ctx: &mut Ctx, h: ColliderHandle, f: Vec3, at: Vec3) {
        if let Some(bh) = ctx.phys.collider(h).and_then(|c| c.parent())
            && bh != self.body {
                let b = &mut ctx.phys.w.bodies[bh];
                if b.is_dynamic() {
                    b.add_force_at_point(f, at, true);
                }
            }
    }

    // ------------------------------------------------------------------ physics
    pub fn step(&mut self, ctx: &mut Ctx, dt: f32) {
        {
            let b = ctx.phys.body_mut(self.body);
            b.reset_forces(true);
            b.reset_torques(true);
        }
        self.read_pose(ctx.phys);
        if !self.alive {
            return;
        }
        let inverted = self.up.y < 0.0;
        let sideways = self.up.y.abs() < 0.35;
        let down = self.up * if inverted { 1.0 } else { -1.0 };
        let drives: Vec<usize> = (0..self.parts.len()).filter(|&i| self.parts[i].attached && self.parts[i].def().category == Category::Drive).collect();
        let m = self.mass;
        let nd = drives.len().max(1) as f32;
        let mut grounded = 0usize;
        let mut on_floor = 0usize; // drives touching the arena, not another bot
        let n_legs = drives.iter().filter(|&&i| self.parts[i].kind == PartKind::Leg).count();
        let leg_mode = n_legs as f32 > drives.len() as f32 / 2.0;
        let steer = self.input.turn * if inverted { -1.0 } else { 1.0 };
        // boost: a short shove of extra torque and top speed, then a cooldown
        self.boost_cd = (self.boost_cd - dt).max(0.0);
        if self.input.boost && self.boost_cd <= 0.0 && !self.halted && self.grounded_frac > 0.0 {
            self.boost_t = Boost::TIME;
            self.boost_cd = Boost::COOLDOWN;
            ctx.sfx.push(Snd::Pneumatic.play());
            ctx.fx.dust_puff(self.bot_world(Vec3::new(0.0, -0.2, -0.6)), 6, None, 0.12, 2.5);
        }
        let boosting = self.boost_t > 0.0;
        self.boost_t = (self.boost_t - dt).max(0.0);
        let vmax = self.top_speed * if boosting { 1.5 } else { 1.0 };
        let car = self.car_steer && !leg_mode;
        let v_fwd = self.linvel(ctx.phys).dot(self.fwd);
        // less lock at speed so it doesn't snap round
        let lock = 0.62 - 0.28 * (v_fwd.abs() / vmax).min(1.0);
        for &i in &drives {
            let p = &mut self.parts[i];
            let want = if car && !self.halted { -steer * lock * p.steer_k } else { 0.0 };
            p.steer += (want - p.steer) * (dt * 10.0).min(1.0);
        }

        for &i in &drives {
            if self.parts[i].kind == PartKind::Leg {
                continue; // legs walk: see step_legs
            }
            let (kind, mount_l, axle_l) = (self.parts[i].kind, self.parts[i].mount, self.parts[i].axle);
            let radius = wheel_radius(kind);
            let max_len = radius + 0.12;
            let mount = self.bot_world(mount_l);
            self.parts[i].grounded = false;
            self.parts[i].dist = max_len;
            if sideways {
                continue;
            }
            let axle_w = self.q * axle_l;
            if axle_w.dot(down).abs() > 0.6 {
                continue; // wheel lying flat
            }
            let po = ctx.part_of;
            let team = self.team;
            let hit = ctx.phys.ray(mount, down, max_len, Some(self.body), &|h| !po.get(&h).is_some_and(|(t, _)| *t == team));
            let share = m / nd;
            let (n, contact, fs, surf);
            if let Some((h, toi, normal)) = hit {
                self.parts[i].dist = toi;
                let comp = max_len - toi;
                let k = share * GRAVITY / 0.08;
                let c = 2.0 * (k * share).sqrt() * 0.55;
                contact = mount + down * toi;
                let vel = ctx.phys.body(self.body).velocity_at_point(mount);
                let f = (k * comp + c * vel.dot(down)).clamp(0.0, share * GRAVITY * 6.0);
                fs = f;
                // resting on another bot: push straight up, so curved tyres and lids
                // don't cradle us and a foe that drives off leaves us behind
                n = if ctx.part_of.contains_key(&h) { -down } else { normal };
                surf = h;
                let support = n * fs;
                ctx.phys.body_mut(self.body).add_force_at_point(support, mount, true);
                self.push_other(ctx, h, -support, contact);
            } else {
                // No ground under the hub (perched on a foe, wedged on a wall edge...):
                // a tyre that is physically touching something still bites on it.
                match self.tyre_contact(ctx, i, mount, dt) {
                    Some((nn, cc, ff, ss)) => {
                        n = nn;
                        contact = cc;
                        fs = ff;
                        surf = ss;
                    }
                    None => continue,
                }
            }
            self.parts[i].grounded = true;
            grounded += 1;
            let on_bot = ctx.part_of.contains_key(&surf);
            if !on_bot {
                on_floor += 1;
            }
            // traction: wheels roll perpendicular to their axle
            let fwd_g = (self.fwd - n * self.fwd.dot(n)).normalize_or_zero();
            let mut roll = axle_w.cross(n);
            if roll.length_squared() < 1e-4 {
                continue;
            }
            roll = roll.normalize();
            if roll.dot(fwd_g) < 0.0 {
                roll = -roll;
            }
            let align = roll.dot(fwd_g);
            let st = self.parts[i].steer;
            if st != 0.0 {
                roll = Quat::from_axis_angle(n, st * if inverted { -1.0 } else { 1.0 }) * roll;
            }
            let lat_g = n.cross(roll).normalize_or_zero();
            let vc = ctx.phys.body(self.body).velocity_at_point(contact);
            let v_f = vc.dot(roll);
            let v_l = vc.dot(lat_g);
            // +side is the driver's left. To turn right the left side drives forward.
            // car steering on the move; parked with only A/D held, pivot like a tank
            let pivot = if car { 0.85 * (1.0 - v_fwd.abs() / 2.0).max(0.0) } else { 0.75 };
            let skid = steer * self.parts[i].side * pivot;
            let cmd = ((self.input.throttle + skid) * align).clamp(-1.0, 1.0);
            let drive_f = if kind == PartKind::BigWheel { 200.0 } else { 165.0 } * self.power_ratio * if boosting { 2.4 } else { 1.0 };
            // handbrake: rear wheels (all wheels on a tank-steerer) lock and slide
            let p = &self.parts[i];
            let rear = !car || p.steer_k < 0.0 || p.mount.z < self.com.z;
            let locked = self.input.brake && rear && !self.halted;
            let mut f_drive = if locked {
                -v_f.signum() * (v_f.abs() * share * 3.0).min(share * GRAVITY * 0.35)
            } else if self.input.brake && !self.halted {
                -v_f * share * 0.05
            } else if cmd.abs() > 0.02 {
                let spd = v_f * cmd.signum();
                cmd * drive_f * (1.0 - spd.max(0.0) / vmax).max(0.0)
            } else {
                -v_f * share * 1.6 // rolling resistance / braking
            };
            let mut f_lat = -v_l * share * 9.0 * if locked { 0.12 } else { 1.0 };
            // standing on a slippery wedge/flipper (or an enemy's lid) loses grip
            let mu = if kind == PartKind::BigWheel { 1.6 } else { 1.35 } * (Self::friction_of(ctx, surf) / 0.45).min(1.0) * if on_bot { 0.45 } else { 1.0 } * if boosting { 1.6 } else { 1.0 };
            let lim = mu * fs * if locked { 0.6 } else { 1.0 }; // sliding grip < rolling grip
            let mag = f_drive.hypot(f_lat);
            if mag > lim {
                f_drive *= lim / mag;
                f_lat *= lim / mag;
            }
            let f = roll * f_drive + lat_g * f_lat;
            let ap = contact.lerp(mount, 0.5);
            ctx.phys.body_mut(self.body).add_force_at_point(f, ap, true);
            // whatever we're driving on gets shoved the other way
            self.push_other(ctx, surf, -f, contact);
            // spin direction depends on which way the axle points
            let s = axle_w.cross(roll).dot(n).signum();
            self.parts[i].wheel_angle += (if s == 0.0 { 1.0 } else { s }) * v_f / radius * dt;
        }
        let legs: Vec<usize> = drives.iter().copied().filter(|&i| self.parts[i].kind == PartKind::Leg).collect();
        if !legs.is_empty() {
            let (g, f) = self.step_legs(ctx, &legs, dt, inverted || sideways, boosting, legs.len() as f32 / drives.len() as f32);
            grounded += g;
            on_floor += f;
        }
        // wheels in the air still spin up with the throttle
        for &i in &drives {
            let p = &mut self.parts[i];
            if !p.grounded && p.kind != PartKind::Leg && !self.halted {
                let cmd = (self.input.throttle + if car { 0.0 } else { steer * p.side * 0.75 }).clamp(-1.0, 1.0);
                p.wheel_angle += cmd * 18.0 * self.power_ratio * dt * if p.side > 0.0 { 1.0 } else { -1.0 };
            }
        }
        self.grounded_frac = if drives.is_empty() { 0.0 } else { grounded as f32 / drives.len() as f32 };

        // Beached (belly on a foe or a ledge, no drive touching anything): scrabble
        // at whatever we can, so you can wriggle off rather than sit there forever.
        if on_floor == 0 && !sideways && !inverted && !self.halted && (self.input.throttle.abs() > 0.02 || self.input.turn.abs() > 0.02) && self.touching_anything(ctx) {
            let mut f = self.fwd;
            f.y = 0.0;
            let f = f.normalize_or_zero() * self.input.throttle * m * GRAVITY * 0.9 * self.power_ratio;
            let b = ctx.phys.body_mut(self.body);
            b.add_force(Vec3::new(f.x, m * GRAVITY * 0.3, f.z), true);
            b.add_torque(Vec3::new(0.0, -self.input.turn * m * 1.6 * self.power_ratio, 0.0), true);
        }

        // Yaw assist + damping so skid steering feels responsive but not floaty.
        if grounded > 0 && !leg_mode {
            let yaw_rate = ctx.phys.body(self.body).angvel().y;
            let mut rate = 4.2;
            let mut dir = 1.0;
            if car {
                // like a car: turn rate grows with speed and flips in reverse; a slow
                // shuffle-pivot at a standstill so you can never get properly stuck
                rate = (3.2 * (1.0 - v_fwd.abs() / 2.0).max(0.0)).max((1.2 + 0.55 * v_fwd.abs()).min(3.4));
                dir = if v_fwd > 0.3 { 1.0 } else if v_fwd < -0.3 || self.input.throttle < -0.1 { -1.0 } else { 1.0 };
            }
            if car && self.input.brake && v_fwd.abs() > 0.6 {
                rate *= 1.9;
            }
            let want = -self.input.turn * rate * dir * self.power_ratio * self.agility;
            let tq = (want - yaw_rate) * m * 0.35 * self.grounded_frac;
            ctx.phys.body_mut(self.body).add_torque(Vec3::new(0.0, tq, 0.0), true);
        }

        // Self-righting wiggle when stuck on its side.
        if sideways {
            self.side_time += dt;
            if self.side_time > 1.6 {
                self.side_time = 0.0;
                let axis = self.up.cross(Vec3::Y).normalize_or_zero();
                let b = ctx.phys.body_mut(self.body);
                b.apply_impulse(Vec3::new(0.0, m * 2.6, 0.0), true);
                b.apply_torque_impulse(Vec3::new(axis.x * m * 0.9, 0.0, axis.z * m * 0.9), true);
                ctx.fx.dust(self.pos.with_y(0.05), 8);
                ctx.sfx.push(Snd::Thud.at(0.3));
            }
        } else {
            self.side_time = 0.0;
        }
        if inverted {
            self.invert_time += dt;
        } else {
            self.invert_time = 0.0;
        }
        // walkers on their back kick like a beetle until they roll onto a side
        // (the side wiggle above then stands them up); wheels can drive inverted
        if leg_mode && self.up.y < -0.35 && self.invert_time > 1.2 && !self.halted {
            self.invert_time = 0.4;
            let s = if rng::chance(0.5) { 1.0 } else { -1.0 };
            let roll = self.fwd;
            let b = ctx.phys.body_mut(self.body);
            b.apply_impulse(Vec3::new(0.0, m * 2.2, 0.0), true);
            b.apply_torque_impulse(Vec3::new(roll.x * m * 1.1 * s, 0.0, roll.z * m * 1.1 * s), true);
            ctx.fx.dust(self.pos.with_y(0.05), 8);
            ctx.sfx.push(Snd::Thud.at(0.3));
        }
        self.step_weapons(ctx, dt, inverted);
    }

    /// Where a leg wants its foot when standing still: under the hip, splayed
    /// out the way the leg points. Mount a leg on a side and it sprawls like a
    /// spider; underneath and it stands tall like a stilt-walker.
    fn leg_home(&self, p: &RPart, hip: Vec3) -> Vec3 {
        let mut out = self.dir_world(p, Vec3::Z);
        out.y = 0.0;
        let splay = (out.length() * 1.4).min(1.0);
        hip + out.normalize_or_zero() * 0.34 * splay
    }

    /// Inverse-kinematic walking. Feet are planted in the world; the body is held
    /// up by springs at the planted feet and pushed/turned by them, so a wider
    /// stance is steadier, fewer legs wobble, and where you bolted each leg
    /// shapes the gait. Steps are planned from where the body is heading and
    /// turning, so turning while walking just works.
    fn step_legs(&mut self, ctx: &mut Ctx, legs: &[usize], dt: f32, upset: bool, boosting: bool, share: f32) -> (usize, usize) {
        let m = self.mass;
        let reach = 0.84;
        let v = self.linvel(ctx.phys);
        let vh = v.with_y(0.0);
        let speed = vh.length();
        let (mut grounded, mut on_floor) = (0usize, 0usize);
        let step_dur = (0.2 - speed * 0.018).clamp(0.11, 0.2);
        let mut wants = [false, false];
        // 1. gait: plan and move feet
        for &i in legs {
            let hip = self.bot_world(self.parts[i].mount);
            if upset {
                // flipped / on its side: legs flail in the air
                let p = &self.parts[i];
                let mut dangle = hip + self.up * if self.up.y < 0.0 { 0.45 } else { -0.45 } + self.dir_world(p, Vec3::Z) * 0.25;
                dangle.y += (ctx.time * 14.0 + p.gait as f32 * 3.0).sin() * 0.08;
                let p = &mut self.parts[i];
                let f = p.foot.unwrap_or(dangle);
                p.foot = Some(f.lerp(dangle, (dt * 10.0).min(1.0)));
                p.step_t = 1.0;
                p.planted = false;
                p.grounded = false;
                continue;
            }
            // aim the next footfall ahead along the hip's motion (incl. turning)
            let v_hip = ctx.phys.body(self.body).velocity_at_point(hip).with_y(0.0);
            let mut target = self.leg_home(&self.parts[i], hip) + v_hip * step_dur * 1.1;
            target.y = match self.ground_at(ctx, target.x, hip.y + 0.15, target.z, LEG_LEN + 0.6) {
                Some((y, _)) => y,
                None => hip.y - LEG_LEN - 0.3,
            };
            {
                let p = &mut self.parts[i];
                // first touch-down (also after recovering from a flip or a big knock)
                if p.foot.is_none_or(|f| f.distance(hip) > reach * 2.0) {
                    p.foot = Some(target);
                    p.foot_from = target;
                    p.foot_to = target;
                    p.step_t = 1.0;
                }
            }
            let group_stepping = |s: &Self, g: usize| legs.iter().any(|&j| s.parts[j].gait == g && s.parts[j].step_t < 1.0);
            if self.parts[i].step_t < 1.0 {
                let p = &mut self.parts[i];
                p.foot_to = p.foot_to.lerp(target, 0.25); // steer the swing towards the moving target
                p.step_t = (p.step_t + dt / step_dur).min(1.0);
                let k = p.step_t * p.step_t * (3.0 - 2.0 * p.step_t);
                let mut f = p.foot_from.lerp(p.foot_to, k);
                f.y += (p.step_t * PI).sin() * (0.12 + (speed * 0.03).min(0.12));
                p.foot = Some(f);
                if p.step_t >= 1.0 {
                    let (fx_, fy_, fz_) = (f.x, f.y, f.z);
                    if let Some((gy, _)) = self.ground_at(ctx, fx_, fy_ + 0.3, fz_, 0.6) {
                        self.parts[i].foot = Some(Vec3::new(fx_, gy, fz_));
                    }
                    let ff = self.parts[i].foot.unwrap();
                    ctx.fx.dust_puff(ff + Vec3::Y * 0.02, 2, None, 0.07, 0.6);
                    if self.team == 0 {
                        ctx.sfx.push(Snd::Scrape.play());
                    }
                }
            } else {
                let f = self.parts[i].foot.unwrap();
                let drift = (f.x - target.x).hypot(f.z - target.z);
                let strain = f.distance(hip) > reach - 0.04;
                let lost = self.ground_at(ctx, f.x, f.y + 0.25, f.z, 0.4).is_none();
                let gait = self.parts[i].gait;
                let other = 1 - gait;
                let others_planted = legs.iter().any(|&j| self.parts[j].gait == other && self.parts[j].planted) || legs.len() == 1;
                // groups take turns: the group that just stepped waits for the other
                // one (unless the other has nothing to do), so no leg gets left behind
                let joining = self.last_gait == gait && ctx.time - self.gait_t < step_dur * 0.4;
                let my_turn = self.last_gait != gait || joining || !self.gait_wants[other];
                if drift > 0.2 || strain {
                    wants[gait] = true;
                }
                if (drift > 0.2 || strain || lost) && !group_stepping(self, other) && ((others_planted && my_turn) || lost) {
                    if self.last_gait != gait || !joining {
                        self.gait_t = ctx.time;
                    }
                    self.last_gait = gait;
                    let p = &mut self.parts[i];
                    p.foot_from = f;
                    p.foot_to = target;
                    p.step_t = 0.0;
                }
            }
            let f = self.parts[i].foot.unwrap();
            let on = self.parts[i].step_t >= 1.0 && self.ground_at(ctx, f.x, f.y + 0.25, f.z, 0.4).is_some();
            let p = &mut self.parts[i];
            p.planted = on;
            p.grounded = p.planted || p.step_t < 1.0;
            if p.grounded {
                grounded += 1;
            }
        }
        self.gait_wants = wants;
        if upset || self.halted {
            return (grounded, on_floor);
        }

        // 2. support springs at the planted feet
        let stance: Vec<usize> = legs.iter().copied().filter(|&i| self.parts[i].planted).collect();
        if stance.is_empty() {
            return (grounded, on_floor);
        }
        let ns = stance.len() as f32;
        let each = m * GRAVITY / ns;
        let rest_len = LEG_LEN - 0.08;
        let k = each / 0.12;
        let c = 2.0 * (k * (m / ns)).sqrt() * 0.6;
        let (mut normal, mut mu) = (0.0f32, 0.0f32);
        for &i in &stance {
            let hip = self.bot_world(self.parts[i].mount);
            let f = self.parts[i].foot.unwrap();
            let Some((gy, surf)) = self.ground_at(ctx, f.x, f.y + 0.25, f.z, 0.4) else { continue };
            let h = hip.y - gy;
            let vy = ctx.phys.body(self.body).velocity_at_point(hip).y;
            let fs = (k * (rest_len + 0.08 - h) - c * vy).clamp(0.0, each * 5.0);
            // pushed up at the foot: the further out the feet, the steadier the bot
            ctx.phys.body_mut(self.body).add_force_at_point(Vec3::new(0.0, fs, 0.0), Vec3::new(f.x, hip.y, f.z), true);
            let on_bot = ctx.part_of.contains_key(&surf);
            if !on_bot {
                on_floor += 1;
            }
            normal += fs;
            mu += 2.4 * (Self::friction_of(ctx, surf) / 0.45).min(1.0) * if on_bot { 0.45 } else { 1.0 } * fs;
        }
        // 3. walk: feet push the body towards the wanted velocity and yaw rate
        let bk = if boosting { 1.6 } else { 1.0 };
        let vmax = self.top_speed * 0.62 * bk;
        let fwd_h = self.fwd.with_y(0.0).normalize_or_zero();
        let want = fwd_h * self.input.throttle * vmax; // top_speed already counts power
        let lim = mu * bk * share;
        let mut f = (want - vh) * m * if boosting { 14.0 } else { 8.0 };
        if f.length() > lim {
            f = f.normalize_or_zero() * lim;
        }
        let b = ctx.phys.body_mut(self.body);
        b.add_force(Vec3::new(f.x, 0.0, f.z), true);
        let inertia = b.mass_properties().local_mprops.principal_inertia();
        let want_yaw = -self.input.turn * 3.2 * self.agility;
        let tq = ((want_yaw - b.angvel().y) * inertia.y * 12.0).clamp(-lim * 0.6, lim * 0.6);
        b.add_torque(Vec3::new(0.0, tq * share, 0.0), true);
        // balance: the planted legs hold the body level, but only as hard as the
        // weight × stance width allows, so wide sprawled bots are steady and narrow
        // or one-legged ones wobble and can be shoved over
        let com = b.center_of_mass();
        let mut lever = 0.0;
        for &i in &stance {
            let f = self.parts[i].foot.unwrap();
            lever += (f.x - com.x).hypot(f.z - com.z);
        }
        let lever = (lever / ns).max(0.12);
        let lean = self.up.cross(Vec3::Y);
        let av = b.angvel();
        let ih = (inertia.x + inertia.z) / 2.0;
        let max_tq = normal * lever * 0.9 * share;
        let tx = (ih * (lean.x * 90.0 - av.x * 14.0)).clamp(-max_tq, max_tq);
        let tz = (ih * (lean.z * 90.0 - av.z * 14.0)).clamp(-max_tq, max_tq);
        b.add_torque(Vec3::new(tx, 0.0, tz), true);
        (grounded, on_floor)
    }

    /// Contact between a wheel's tyre and anything that isn't us: surface normal
    /// (pointing at the hub), contact point, normal force and the touched collider.
    fn tyre_contact(&self, ctx: &Ctx, i: usize, hub: Vec3, dt: f32) -> Option<(Vec3, Vec3, f32, ColliderHandle)> {
        let mut best: Option<(Vec3, Vec3, f32, ColliderHandle)> = None;
        let floor_bite = self.mass * GRAVITY * 0.25 / (self.parts.len() as f32 / 4.0).max(1.0);
        for &wc in &self.parts[i].colliders {
            for (other, pt, nrm, imp) in ctx.phys.contacts_of(wc) {
                if self.mine(ctx.part_of, other) {
                    continue;
                }
                let n = if nrm.dot(hub - pt) < 0.0 { -nrm } else { nrm };
                // at least a little bite even when the solver reports a feather touch
                let fs = (imp / dt.max(1e-3)).max(floor_bite).min(self.mass * GRAVITY * 1.5);
                if best.is_none_or(|b| fs > b.2) {
                    best = Some((n, pt, fs, other));
                }
            }
        }
        best
    }

    /// Any of our colliders resting on something that isn't us.
    fn touching_anything(&self, ctx: &Ctx) -> bool {
        self.parts.iter().filter(|p| p.attached).flat_map(|p| p.colliders.iter()).any(|&c| ctx.phys.contacts_of(c).iter().any(|(o, ..)| !self.mine(ctx.part_of, *o)))
    }

    /// First attached part of another robot overlapping a ball at `at`.
    fn robot_at(&self, ctx: &Ctx, at: Vec3, r: f32) -> Option<(usize, usize)> {
        ctx.phys.overlaps(Pose::from_translation(at), &Ball::new(r)).into_iter().find_map(|h| ctx.part_of.get(&h).copied().filter(|(t, _)| *t != self.team))
    }

    /// Floor or wall overlapping a ball at `at`.
    fn solid_at(&self, ctx: &Ctx, at: Vec3, r: f32) -> bool {
        ctx.phys.overlaps(Pose::from_translation(at), &Ball::new(r)).into_iter().any(|h| {
            ctx.phys.collider(h).is_some_and(|c| c.collision_groups().memberships.bits() & (grp::GROUND | grp::WALL) != 0)
        })
    }

    fn fires(&self, p: &RPart) -> bool {
        match self.input.fire_slots {
            Some(s) => s[p.slot],
            None => self.input.fire,
        }
    }

    fn step_weapons(&mut self, ctx: &mut Ctx, dt: f32, inverted: bool) {
        if self.halted {
            return;
        }
        for i in 0..self.parts.len() {
            if !self.parts[i].attached {
                continue;
            }
            self.parts[i].cooldown = (self.parts[i].cooldown - dt).max(0.0);
            match self.parts[i].kind {
                PartKind::Spinner => self.step_spinner(ctx, i, dt),
                PartKind::Hammer => {
                    self.step_hammer(ctx, i, dt, inverted);
                    Self::pose_hammer(ctx.phys, &self.parts[i], self.parts[i].angle);
                }
                PartKind::Flipper => self.step_flipper(ctx, i, dt),
                _ => {}
            }
        }
    }

    fn step_spinner(&mut self, ctx: &mut Ctx, i: usize, dt: f32) {
        let p = &mut self.parts[i];
        p.spin = (p.spin + dt * 0.45 * self.power_ratio).min(1.0);
        p.angle += p.spin * 55.0 * dt;
        if p.spin < 0.2 {
            return;
        }
        let Some(&disc) = p.colliders.get(1) else { return };
        let found = ctx.phys.contacts_of(disc).into_iter().find(|(o, ..)| !self.mine(ctx.part_of, *o));
        let Some((other, hit_pt, _, _)) = found else { return };
        let p = &self.parts[i];
        let e = p.spin * p.spin;
        let centre = self.part_world(p, Vec3::new(0.0, 0.0, 0.2));
        let axis = self.dir_world(p, Vec3::Z);
        let mut radial = hit_pt - centre;
        radial = (radial - axis * radial.dot(axis)).normalize_or_zero();
        let tangent = axis.cross(radial).normalize_or_zero();
        let dir = (radial * 0.8 + tangent + Vec3::Y * 0.4).normalize_or_zero();
        if let Some(&op) = ctx.part_of.get(&other) {
            let j = 165.0 * e;
            let ob = ctx.phys.collider(other).and_then(|c| c.parent());
            if let Some(ob) = ob {
                ctx.phys.body_mut(ob).apply_impulse_at_point(dir * j, hit_pt, true);
            }
            let back = radial * (-j * 0.45) + tangent * (-j * 0.25) + Vec3::Y * j * 0.1;
            ctx.phys.body_mut(self.body).apply_impulse_at_point(back, centre, true);
            ctx.fx.spark_burst(hit_pt, (30.0 + 50.0 * e) as usize, Some(dir), 10.0, 1.0);
            ctx.fx.bolt_burst(hit_pt, (3.0 + 8.0 * e) as usize, Some(dir));
            ctx.sfx.push(Snd::SpinnerHit.at(0.4 + e * 0.6));
            ctx.ev.push(Ev::HitStop(0.05 + 0.09 * e));
            ctx.fx.add_shake(0.35 + e * 0.5);
            ctx.ev.push(Ev::Hit { att: Some(self.team), tgt: op, dmg: 40.0 * e, pt: hit_pt, big: e > 0.45, word: None });
        } else {
            // wall / floor / debris: bounce off it and grind
            let j = 90.0 * e;
            ctx.phys.body_mut(self.body).apply_impulse_at_point(Vec3::new(-radial.x * j, 10.0 * e, -radial.z * j), centre, true);
            ctx.fx.spark_burst(hit_pt, (15.0 + 20.0 * e) as usize, Some(-radial), 7.0, 1.0);
            ctx.sfx.push(Snd::Grind.at(0.5 + e * 0.5));
            ctx.fx.add_shake(0.15);
        }
        self.parts[i].spin *= 0.35;
    }

    fn step_hammer(&mut self, ctx: &mut Ctx, i: usize, dt: f32, inverted: bool) {
        const SWING: f32 = 0.12;
        const HOLD: f32 = 0.12;
        const RAISE: f32 = 0.5;
        let (rest, strike) = (HAMMER.rest, HAMMER.strike);
        if self.parts[i].anim < 0.0 {
            self.parts[i].angle = rest;
            if self.fires(&self.parts[i]) && self.parts[i].cooldown <= 0.0 {
                let p = &mut self.parts[i];
                p.anim = 0.0;
                p.struck = false;
                p.stop_at = None;
                ctx.sfx.push(Snd::Whoosh.play());
                // upside down with the head on the floor: the arm levers us over
                let head = self.part_world(&self.parts[i], hammer_head(rest));
                if inverted && self.solid_at(ctx, head, 0.25) {
                    self.parts[i].struck = true;
                    self.slam(ctx, head, true);
                }
            }
            return;
        }
        let prev = self.parts[i].angle;
        {
            let p = &mut self.parts[i];
            p.anim += dt * self.power_ratio;
            let t = p.anim;
            let peak = p.stop_at.unwrap_or(strike);
            if p.stop_at.is_none() && t < SWING {
                p.angle = rest + (strike - rest) * (t / SWING).powi(2);
            } else if t < SWING + HOLD {
                p.angle = peak;
            } else if t < SWING + HOLD + RAISE {
                p.angle = peak + (rest - peak) * ((t - SWING - HOLD) / RAISE);
            } else {
                p.angle = rest;
                p.anim = -1.0;
                p.cooldown = 0.45;
            }
            if p.struck {
                return;
            }
        }
        // Sweep the head along the arc it travelled this frame: the first robot it
        // meets stops the swing and takes the blow.
        let to = self.parts[i].angle.min(strike);
        let steps = ((to - prev) / 0.1).ceil().max(1.0) as usize;
        if to > prev {
            for s in 1..=steps {
                let a = prev + (to - prev) * s as f32 / steps as f32;
                let head = self.part_world(&self.parts[i], hammer_head(a));
                let op = self.robot_at(ctx, head, 0.24);
                let Some(op) = op else {
                    if !self.solid_at(ctx, head, 0.13) {
                        continue;
                    }
                    // head met the floor or a wall: stop dead and slam
                    let p = &mut self.parts[i];
                    p.struck = true;
                    p.stop_at = Some(a);
                    p.angle = a;
                    p.anim = p.anim.max(SWING);
                    self.slam(ctx, head, inverted);
                    break;
                };
                let p = &mut self.parts[i];
                p.struck = true;
                p.stop_at = Some(a);
                p.angle = a;
                p.anim = p.anim.max(SWING); // go straight into the hold
                // direction the head is travelling at impact
                let swing = self.dir_world(&self.parts[i], Vec3::new(0.0, a.cos(), -a.sin()));
                let j = 95.0 * self.power_ratio;
                let dn = swing * j;
                let target_body = ctx.phys.collider(self.parts_collider_of(ctx, op)).and_then(|c| c.parent());
                if let Some(tb) = target_body {
                    ctx.phys.body_mut(tb).apply_impulse_at_point(dn, head, true);
                }
                ctx.phys.body_mut(self.body).apply_impulse_at_point(-dn * 0.35, head, true);
                ctx.fx.spark_burst(head, 45, Some(-swing), 8.0, 1.4);
                ctx.fx.bolt_burst(head, 6, None);
                ctx.sfx.extend([Snd::Clang.play(), Snd::Thud.at(0.9)]);
                ctx.ev.push(Ev::HitStop(0.09));
                ctx.fx.add_shake(0.6);
                ctx.ev.push(Ev::Hit { att: Some(self.team), tgt: op, dmg: 32.0 * self.power_ratio, pt: head, big: true, word: None });
                return;
            }
        }
        if !self.parts[i].struck && self.parts[i].anim >= SWING {
            // reached the bottom of the swing in free air
            self.parts[i].struck = true;
            let head = self.part_world(&self.parts[i], hammer_head(strike));
            if head.y < 0.35 {
                self.slam(ctx, head, inverted);
            }
        }
    }

    /// Any collider belonging to the given (team, part) (to find its body).
    fn parts_collider_of(&self, ctx: &Ctx, op: (usize, usize)) -> ColliderHandle {
        ctx.part_of.iter().find(|(_, v)| **v == op).map(|(h, _)| *h).unwrap_or(ColliderHandle::invalid())
    }

    fn slam(&mut self, ctx: &mut Ctx, head: Vec3, inverted: bool) {
        ctx.fx.dust(head.with_y(0.05), 10);
        ctx.fx.spark_burst(head, 10, Some(Vec3::Y), 5.0, 1.0);
        ctx.sfx.push(Snd::Thud.at(0.5));
        ctx.fx.add_shake(0.2);
        // the floor always pushes back up, so an upside-down slam can self-right
        let b = ctx.phys.body_mut(self.body);
        b.apply_impulse_at_point(Vec3::new(0.0, if inverted { 110.0 } else { 55.0 }, 0.0), head, true);
        if inverted {
            // ...and roll us over the side the hammer is on
            let r = (head - self.pos).with_y(0.0);
            let mut axis = if r.length_squared() > 0.01 { Vec3::Y.cross(r) } else { self.up.cross(Vec3::Y) };
            if axis.length_squared() < 1e-4 {
                axis = self.fwd.with_y(0.0);
            }
            b.apply_torque_impulse(axis.normalize_or_zero() * -self.mass * 0.9, true);
        }
    }

    fn step_flipper(&mut self, ctx: &mut Ctx, i: usize, dt: f32) {
        let (rest, upa) = (FLIP.rest, FLIP.fire);
        {
            let p = &mut self.parts[i];
            if p.anim >= 0.0 {
                p.anim += dt;
                let t = p.anim;
                p.angle = if t < 0.07 {
                    rest + (upa - rest) * (t / 0.07)
                } else if t < 0.3 {
                    upa
                } else if t < 0.9 {
                    upa + (rest - upa) * ((t - 0.3) / 0.6)
                } else {
                    p.anim = -1.0;
                    rest
                };
                return;
            }
            p.angle = rest;
        }
        if !self.fires(&self.parts[i]) || self.parts[i].cooldown > 0.0 {
            return;
        }
        self.parts[i].anim = 0.0;
        self.parts[i].cooldown = 1.9;
        ctx.sfx.push(Snd::Pneumatic.play());
        // everything sitting on / just above the plate
        let p = &self.parts[i];
        let centre = self.part_world(p, Vec3::new(0.0, FLIP.hinge_y + FLIP.len * 0.55, 0.3));
        let wq = self.q * p.quat;
        let mut victims: Vec<((usize, usize), Vec3, RigidBodyHandle)> = vec![];
        for h in ctx.phys.overlaps(Pose::from_parts(centre, wq), &rapier3d::prelude::Cuboid::new(Vec3::new(0.32, FLIP.len * 0.55, 0.4))) {
            if let Some(&op) = ctx.part_of.get(&h)
                && op.0 != self.team && !victims.iter().any(|v| v.0 .0 == op.0)
                    && let Some(c) = ctx.phys.collider(h)
                        && let Some(bh) = c.parent() {
                            victims.push((op, c.translation(), bh));
                        }
        }
        let launch = self.dir_world(p, Vec3::Z);
        let along = self.dir_world(p, Vec3::Y);
        let n_vict = victims.len();
        for (op, at, bh) in victims {
            let j = 170.0 * self.power_ratio; // fixed impulse: heavy bots fly lower
            let b = ctx.phys.body_mut(bh);
            b.apply_impulse_at_point(launch * j + along * j * 0.3, at, true);
            // cap the launch so flips are dramatic but stay inside the arena
            let lv = b.linvel();
            if lv.y > 8.5 {
                b.set_linvel(Vec3::new(lv.x, 8.5, lv.z), true);
            }
            ctx.ev.push(Ev::Hit { att: Some(self.team), tgt: op, dmg: 12.0, pt: at, big: true, word: None });
            ctx.fx.add_shake(0.5);
            ctx.ev.push(Ev::HitStop(0.06));
        }
        // a flipper facing the floor kicks the bot over (self-righting)
        if launch.y < -0.3 {
            let tip = self.part_world(&self.parts[i], Vec3::new(0.0, FLIP.hinge_y + FLIP.len, 0.0));
            ctx.phys.body_mut(self.body).apply_impulse_at_point(launch * -self.mass * 6.5, tip, true);
            ctx.fx.dust(tip.with_y(0.05), 12);
        } else if n_vict > 0 {
            ctx.phys.body_mut(self.body).apply_impulse(launch * -60.0, true);
        }
    }

    // ------------------------------------------------------------------ damage
    fn refresh_damage_look(&mut self, phys: &mut Phys, i: usize, commands: &mut Commands, shared: &Mats) {
        let p = &mut self.parts[i];
        let f = p.hp / p.max_hp;
        if f < 0.6 {
            let a = (1.0 - f) * if p.kind == PartKind::Armour { 0.35 } else { 0.12 };
            p.loosen = Quat::from_euler(EulerRot::XYZ, rng::signed() * 0.5 * a, rng::signed() * 0.5 * a, rng::signed() * 0.5 * a);
            if p.attached {
                Self::place_colliders(phys, p);
                if p.kind == PartKind::Hammer {
                    Self::pose_hammer(phys, p, p.angle);
                }
            }
        }
        if f < 0.35 && !p.scorched {
            p.scorched = true;
            for e in [Some(p.ent), p.spin_ent.map(|x| x.0), p.arm_ent.map(|x| x.0)].into_iter().flatten() {
                commands.entity(e).insert(MeshMaterial3d(shared.scorch.clone()));
            }
        }
    }

    /// Returns true if the core died.
    pub fn damage(&mut self, dc: &mut DetachCtx, i: usize, dmg: f32, dir: Option<Vec3>) {
        if !self.parts[i].attached || !self.alive {
            return;
        }
        {
            let p = &mut self.parts[i];
            p.hp -= dmg;
            p.punch = (p.punch + 0.4 + dmg / 40.0).min(1.0);
        }
        self.damage_taken += dmg;
        self.mood = 2.5;
        self.refresh_damage_look(dc.phys, i, dc.commands, dc.shared);
        if self.parts[i].hp <= 0.0 {
            self.parts[i].hp = 0.0;
            if i == self.core {
                self.destroy(dc);
            } else {
                self.detach(dc, i, dir, false);
            }
        }
    }

    pub fn detach(&mut self, dc: &mut DetachCtx, i: usize, dir: Option<Vec3>, quiet: bool) {
        if !self.parts[i].attached {
            return;
        }
        self.parts[i].attached = false;
        let kind = self.parts[i].kind;
        let b = kind.def().bounds;
        let wp = self.part_world(&self.parts[i], b.c);
        let vel = dc.phys.body(self.body).velocity_at_point(wp);
        for c in std::mem::take(&mut self.parts[i].colliders) {
            dc.part_of.remove(&c);
            dc.phys.w.remove_collider(c);
        }
        let wq = self.q * self.parts[i].quat;
        let origin = self.part_world(&self.parts[i], Vec3::ZERO);
        let db = dc.phys.w.insert_body(RigidBodyBuilder::dynamic().pose(Pose::from_parts(origin, wq)).linear_damping(0.1).angular_damping(0.3).ccd_enabled(true));
        let is_wheel = kind.is_wheel();
        let shape = if is_wheel {
            ColliderBuilder::cylinder(0.12, wheel_radius(kind)).position(Pose::from_parts(Vec3::new(0.0, 0.0, b.c.z), Y_TO_Z))
        } else {
            ColliderBuilder::cuboid(b.h.x.max(0.06), b.h.y.max(0.06), b.h.z.max(0.06)).translation(b.c)
        };
        dc.phys.w.insert_collider(shape.mass((kind.def().mass * 0.6).max(0.6)).friction(0.6).restitution(0.3).collision_groups(groups(grp::DEBRIS, grp::ALL)), Some(db));
        let kick = dir.unwrap_or_else(|| (wp - self.pos).with_y(0.0).normalize_or_zero());
        let spd = if quiet { 1.5 } else { 4.0 };
        let dbody = dc.phys.body_mut(db);
        dbody.set_linvel(Vec3::new(vel.x + kick.x * spd, vel.y + 3.0 + rng::rand() * 2.0, vel.z + kick.z * spd), true);
        let spin_axis = if is_wheel { self.dir_world(&self.parts[i], Vec3::Z) * 25.0 } else { Vec3::new(rng::signed() * 7.0, rng::signed() * 7.0, rng::signed() * 7.0) };
        dbody.set_angvel(spin_axis, true);
        // move visuals to world
        let p = &mut self.parts[i];
        dc.commands.entity(p.ent).remove::<ChildOf>().insert(Transform { translation: origin, rotation: wq, ..default() });
        if let Some(l) = p.legs.take() {
            // dangle the thigh off the hip; the shin and foot go with it as one piece
            dc.commands.entity(l.upper).insert(Transform { translation: Vec3::new(0.0, 0.0, 0.2), rotation: Quat::from_rotation_x(0.6), ..default() });
            dc.commands.entity(p.ent).add_child(l.upper);
            dc.commands.entity(l.upper).add_child(l.lower);
            dc.commands.entity(l.lower).insert(Transform { translation: Vec3::new(0.0, 0.0, 0.42), rotation: Quat::from_rotation_x(0.9), ..default() });
            dc.commands.entity(l.lower).add_child(l.foot);
            dc.commands.entity(l.foot).insert(Transform { translation: Vec3::new(0.0, 0.0, 0.44), rotation: Quat::from_rotation_x(-1.5), ..default() });
        }
        dc.debris.push(Debris { body: db, ent: p.ent, born: dc.time, smoke_t: 0.0 });
        if !quiet {
            dc.sfx.push(Snd::Snap.play());
            dc.fx.spark_burst(wp, 45, Some(kick), 7.0, 1.3);
            dc.fx.bolt_burst(wp, 12, Some(kick));
            dc.fx.shard_burst(wp, 22, self.paint, Some(kick), 6.0);
            dc.fx.smoke_puff(wp, 8, 0.25, 0.22);
            dc.fx.dust_puff(wp.with_y(0.05), 5, None, 0.12, 2.0);
            dc.fx.add_shake(0.35);
            dc.hit_stop = dc.hit_stop.max(0.05);
        }
        dc.detached.push((self.team, i));
        // cascade: anything hanging off this part falls off too
        let id = self.parts[i].placed.id;
        for j in 0..self.parts.len() {
            if self.parts[j].attached && self.parts[j].placed.parent == Some(id) {
                self.detach(dc, j, Some(kick), true);
            }
        }
    }

    pub fn destroy(&mut self, dc: &mut DetachCtx) {
        if !self.alive {
            return;
        }
        self.alive = false;
        dc.sfx.push(Snd::Explode.play());
        dc.fx.add_shake(1.0);
        dc.hit_stop = dc.hit_stop.max(0.2);
        dc.fx.spark_burst(self.pos, 120, Some(Vec3::Y), 12.0, 1.5);
        dc.fx.smoke_puff(self.pos, 18, 0.15, 0.35);
        for _ in 0..12 {
            dc.fx.fire(self.pos + Vec3::new(rng::signed() * 0.5, 0.0, rng::signed() * 0.5));
        }
        dc.fx.bolt_burst(self.pos, 30, None);
        dc.fx.shard_burst(self.pos, 60, self.paint, Some(Vec3::Y), 9.0);
        for i in 0..self.parts.len() {
            if !self.parts[i].attached || i == self.core {
                continue;
            }
            let d = (self.part_world(&self.parts[i], Vec3::ZERO) - self.pos).with_y(0.4).normalize_or_zero() * 2.0;
            self.detach(dc, i, Some(d), true);
        }
        let b = dc.phys.body_mut(self.body);
        b.apply_impulse(Vec3::new(0.0, self.mass * 6.0, 0.0), true);
        b.apply_torque_impulse(Vec3::new(rng::signed() * 15.0, 10.0, rng::signed() * 15.0), true);
        self.stop_hums(dc.sfx);
    }

    pub fn stop_hums(&self, sfx: &mut Vec<Sfx>) {
        sfx.push(self.motor.stop());
        if let Some(h) = self.spin_hum {
            sfx.push(h.stop());
        }
    }

    // ------------------------------------------------------------------ visuals
    /// Compute every visual transform for this frame. `out` gets (entity,
    /// transform); `mats` gets eye material swaps.
    pub fn sync_visual(&mut self, phys: &Phys, fx: &mut Fx, sfx: &mut Vec<Sfx>, dt: f32, out: &mut Vec<(Entity, Transform)>, mats: &mut Vec<(Entity, Eyes)>) {
        let b = phys.body(self.body);
        out.push((self.root, Transform { translation: b.translation(), rotation: *b.rotation(), ..default() }));
        self.read_pose(phys);
        let inverted = self.up.y < 0.0;
        self.blink -= dt;
        self.mood = (self.mood - dt).max(0.0);
        let mut spin_level: f32 = 0.0;
        let down_bot = Vec3::new(0.0, if inverted { 1.0 } else { -1.0 }, 0.0);
        for i in 0..self.parts.len() {
            if !self.parts[i].attached {
                continue;
            }
            let p = &mut self.parts[i];
            p.punch = (p.punch - dt * 5.0).max(0.0);
            let s = 1.0 + p.punch * 0.14;
            out.push((p.ent, Transform { translation: p.pos, rotation: p.quat * p.loosen, scale: Vec3::splat(s) }));
            if let Some((e, base)) = p.spin_ent {
                if p.kind.is_wheel() {
                    // drop the wheel onto the floor along the bot's down axis
                    // (suspension travel only shows when upright)
                    let radius = wheel_radius(p.kind);
                    let off = if self.up.y > 0.92 { (p.dist - radius).clamp(0.0, 0.12) } else { 0.0 };
                    let inv = p.quat.inverse();
                    let up_l = inv * Vec3::Y;
                    out.push((e, Transform { translation: base + inv * down_bot * off, rotation: Quat::from_axis_angle(up_l, p.steer) * Quat::from_rotation_z(p.wheel_angle), ..default() }));
                } else {
                    out.push((e, Transform { translation: base, rotation: Quat::from_rotation_z(p.angle), ..default() }));
                    spin_level = spin_level.max(p.spin);
                }
            }
            if let Some((e, base)) = p.arm_ent {
                out.push((e, Transform { translation: base, rotation: Quat::from_rotation_x(-p.angle), ..default() }));
            }
            if let Some((e, origin, tip)) = p.whip {
                // squash the antenna along its length so its tip stays above the floor
                let base_w = self.q * (p.quat * origin + p.pos) + self.pos;
                let tip_w = self.q * (p.quat * (origin + tip) + p.pos) + self.pos;
                let mut sy = 1.0;
                if tip_w.y < 0.02 && base_w.y > tip_w.y {
                    sy = ((base_w.y - 0.02) / (base_w.y - tip_w.y)).clamp(0.15, 1.0);
                }
                out.push((e, Transform { translation: origin, scale: Vec3::new(1.0, sy, 1.0), ..default() }));
            }
            // damage smoke
            let f = p.hp / p.max_hp;
            if f < 0.5 && dt > 0.0 {
                p.smoke_t -= dt * if f < 0.25 { 6.0 } else { 2.5 };
                if p.smoke_t <= 0.0 {
                    p.smoke_t = 1.0;
                    let c = p.kind.def().bounds.c;
                    let wp = self.q * (p.quat * c + p.pos) + self.pos;
                    fx.smoke_puff(wp, 1, if f < 0.25 { 0.12 } else { 0.35 }, 0.1);
                    if f < 0.2 {
                        fx.fire(wp);
                    }
                }
            }
            if self.parts[i].legs.is_some() {
                self.pose_leg(i, out);
            }
        }
        // eyes
        let core = self.core;
        if self.alive {
            if self.blink < -3.0 - rng::rand() * 3.0 {
                self.blink = 0.12;
            }
            let closed = self.blink > 0.0;
            for (k, (e, t)) in self.parts[core].eyes.iter().enumerate() {
                let sy = if closed { 0.15 } else if self.mood > 0.0 { 0.55 } else { 1.0 };
                let rz = if self.mood > 0.0 { if k > 0 { -0.5 } else { 0.5 } } else { 0.0 };
                out.push((*e, Transform { translation: t.translation, rotation: Quat::from_rotation_z(rz), scale: Vec3::new(1.0, sy, 1.0) }));
                mats.push((*e, if self.mood > 0.0 { Eyes::Angry } else { Eyes::Calm }));
            }
            sfx.push(self.motor.set((self.input.throttle.abs() + self.input.turn.abs() * 0.5).min(1.0), if self.team == 0 { 1.0 } else { 0.6 }));
            if let Some(h) = self.spin_hum {
                sfx.push(h.set(spin_level, 1.0));
            }
        } else {
            for (e, _) in &self.parts[core].eyes {
                mats.push((*e, Eyes::Dead));
            }
        }
    }

    /// Two-bone IK from the hip to the foot.
    fn pose_leg(&mut self, i: usize, out: &mut Vec<(Entity, Transform)>) {
        let hip = self.bot_world(self.parts[i].mount);
        if self.parts[i].foot.is_none() {
            let h = self.leg_home(&self.parts[i], hip);
            self.parts[i].foot = Some(h.with_y(hip.y - LEG_LEN));
        }
        let p = &self.parts[i];
        let (a, bl) = (0.42f32, 0.44f32);
        let mut foot = p.foot.unwrap();
        let mut to_foot = foot - hip;
        let mut d = to_foot.length();
        if d > a + bl - 0.02 {
            to_foot = to_foot.normalize_or_zero() * (a + bl - 0.02);
            foot = hip + to_foot;
            d = a + bl - 0.02;
        }
        let dir = to_foot.normalize_or(Vec3::NEG_Y);
        let cos_a = ((a * a + d * d - bl * bl) / (2.0 * a * d.max(1e-4))).clamp(-1.0, 1.0);
        // knee: sprawled legs bend up and out like a spider; legs underneath bend
        // backwards like a bird's
        let out_w = self.dir_world(p, Vec3::Z).with_y(0.0);
        let sprawl = (out_w.length() * 1.4).min(1.0);
        let body_up = self.up * if self.up.y >= 0.0 { 1.0 } else { -1.0 };
        let pole = body_up + out_w.normalize_or_zero() * 0.8 * sprawl - self.fwd * 0.9 * (1.0 - sprawl);
        let mut ortho = pole - dir * pole.dot(dir);
        if ortho.length_squared() < 1e-6 {
            ortho = self.up;
        }
        let ortho = ortho.normalize();
        let knee = hip + dir * cos_a * a + ortho * (1.0 - cos_a * cos_a).max(0.0).sqrt() * a;
        let l = p.legs.as_ref().unwrap();
        out.push((l.upper, Transform { translation: hip, rotation: look_z(knee - hip, Vec3::Y), ..default() }));
        out.push((l.lower, Transform { translation: knee, rotation: look_z(foot - knee, Vec3::Y), ..default() }));
        let yaw = self.fwd.x.atan2(self.fwd.z);
        out.push((l.foot, Transform { translation: foot, rotation: Quat::from_rotation_y(yaw), ..default() }));
    }

    pub fn hp_frac(&self) -> f32 {
        let c = &self.parts[self.core];
        c.hp / c.max_hp
    }

    pub fn dispose(&self, phys: &mut Phys, part_of: &mut PartOf, commands: &mut Commands, sfx: &mut Vec<Sfx>) {
        self.stop_hums(sfx);
        for p in &self.parts {
            for c in &p.colliders {
                part_of.remove(c);
            }
            if p.attached
                && let Some(l) = &p.legs {
                    for e in [l.upper, l.lower, l.foot] {
                        commands.entity(e).try_despawn();
                    }
                }
        }
        phys.w.remove_body_with_colliders(self.body, true);
        commands.entity(self.root).try_despawn();
    }
}

pub struct Debris {
    pub body: RigidBodyHandle,
    pub ent: Entity,
    pub born: f32,
    pub smoke_t: f32,
}

/// What detaching/damaging needs from the battle.
pub struct DetachCtx<'a, 'w, 's> {
    pub phys: &'a mut Phys,
    pub part_of: &'a mut PartOf,
    pub fx: &'a mut Fx,
    pub sfx: &'a mut Vec<Sfx>,
    pub commands: &'a mut Commands<'w, 's>,
    pub shared: &'a Mats,
    pub debris: &'a mut Vec<Debris>,
    pub detached: &'a mut Vec<(usize, usize)>,
    pub hit_stop: f32,
    pub time: f32,
}
