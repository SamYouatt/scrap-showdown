//! Robot part models, made in Blender (`art/parts.py`). Part-local convention
//! (see parts.rs): the attach face is at z = 0 and the part extends along +Z;
//! +Y is the part's "up" arrow (the direction a hammer swings / a flipper
//! plate points).
//!
//! Each part is the asset's `body` piece plus optional moving pieces:
//! `spin` (wheel / disc, spins about local Z at its pivot), `arm` (hammer /
//! flipper, rotates about local X at its pivot), `whip` (the antenna),
//! `upper`/`lower`/`foot` (leg segments posed by IK) and `glow_*` eyes.
use crate::art::Art;
use crate::models::Col;
use crate::parts::{PartKind, HAMMER};
use bevy::prelude::*;

pub fn art_name(kind: PartKind) -> &'static str {
    match kind {
        PartKind::Core => "core",
        PartKind::Block => "block",
        PartKind::Slab => "slab",
        PartKind::Girder => "girder",
        PartKind::Armour => "armour",
        PartKind::Battery => "battery",
        PartKind::Wheel => "wheel",
        PartKind::BigWheel => "bigwheel",
        PartKind::Leg => "leg",
        PartKind::Spinner => "spinner",
        PartKind::Hammer => "hammer",
        PartKind::Flipper => "flipper",
        PartKind::Spike => "spike",
        PartKind::Wedge => "wedge",
    }
}

pub fn key(kind: PartKind, group: &str) -> String {
    format!("{}.{group}", art_name(kind))
}

/// How to draw a part.
#[derive(Clone)]
pub struct PartLook {
    pub paint: Col,
    /// battered/scorched textures
    pub worn: bool,
    /// draw everything (eyes included) with this material instead (ghosts)
    pub ghost: Option<Handle<StandardMaterial>>,
    pub eye: Handle<StandardMaterial>,
    pub tip: Handle<StandardMaterial>,
}

pub struct PartEnts {
    pub ent: Entity,
    /// moving piece + its pivot in part space
    pub spin: Option<(Entity, Vec3)>,
    pub arm: Option<(Entity, Vec3)>,
    pub eyes: Vec<(Entity, Transform)>,
    /// antenna entity, its origin, and its tip relative to the origin
    pub whip: Option<(Entity, Vec3, Vec3)>,
    /// upper, lower, foot; spawned unparented at the origin for the caller to pose
    pub legs: Option<[Entity; 3]>,
}

/// Spawn a part's pieces: the body at `t`, moving pieces as its children at
/// their rest pose, leg segments as separate entities.
pub fn spawn_part(commands: &mut Commands, meshes: &mut Assets<Mesh>, art: &Art, kind: PartKind, look: &PartLook, t: Transform, extra: impl Bundle + Clone) -> PartEnts {
    let piece = |commands: &mut Commands, meshes: &mut Assets<Mesh>, group: &str, t: Transform| {
        let k = key(kind, group);
        let p = art.piece(&k);
        let mat = look.ghost.clone().unwrap_or_else(|| if look.worn { p.scorch.clone() } else { p.mat.clone() });
        commands.spawn((Mesh3d(art.mesh(meshes, &k, &[look.paint])), MeshMaterial3d(mat), t * p.xf, extra.clone())).id()
    };
    let has = |group: &str| art.get(&key(kind, group)).is_some();
    let ent = piece(commands, meshes, "body", t);
    let child = |commands: &mut Commands, meshes: &mut Assets<Mesh>, group: &str| {
        let e = piece(commands, meshes, group, Transform::IDENTITY);
        commands.entity(ent).add_child(e);
        (e, art.piece(&key(kind, group)).xf.translation)
    };
    let spin = has("spin").then(|| child(commands, meshes, "spin"));
    let arm = has("arm").then(|| child(commands, meshes, "arm"));
    let whip = has("whip").then(|| {
        let (e, origin) = child(commands, meshes, "whip");
        let tip = art.piece(&key(kind, "glow_tip"));
        let tip_rel = tip.xf.translation - origin;
        let mat = look.ghost.clone().unwrap_or_else(|| look.tip.clone());
        let te = commands.spawn((art.glow(&key(kind, "glow_tip"), mat, Transform::from_translation(-origin)), extra.clone())).id();
        commands.entity(e).add_child(te);
        (e, origin, tip_rel)
    });
    let mut eyes = vec![];
    for side in ["glow_eye_l", "glow_eye_r"] {
        let k = key(kind, side);
        if let Some(p) = art.get(&k) {
            let mat = look.ghost.clone().unwrap_or_else(|| look.eye.clone());
            let e = commands.spawn((art.glow(&k, mat, Transform::IDENTITY), extra.clone())).id();
            commands.entity(ent).add_child(e);
            eyes.push((e, p.xf));
        }
    }
    let legs = has("upper").then(|| ["upper", "lower", "foot"].map(|g| piece(commands, meshes, g, Transform::IDENTITY)));
    PartEnts { ent, spin, arm, eyes, whip, legs }
}

fn pts(art: &Art, kind: PartKind, group: &str) -> Vec<Vec3> {
    art.get(&key(kind, group)).map(|p| p.pts.iter().map(|q| p.xf.transform_point(*q)).collect()).unwrap_or_default()
}

/// Hammer arm as two point clouds (shaft, head) for convex hull colliders,
/// in the arm's own frame (origin = pivot, shaft along +Z).
pub fn hammer_arm_hulls(art: &Art) -> (Vec<Vec3>, Vec<Vec3>) {
    let (mut shaft, mut head) = (vec![], vec![]);
    for p in &art.piece(&key(PartKind::Hammer, "arm")).pts {
        if p.z > HAMMER.len - 0.16 { head.push(*p) } else { shaft.push(*p) }
    }
    (dedupe(shaft), dedupe(head))
}

/// Points rounded to a 1 cm grid: plenty for a hull, far fewer than a bevelled mesh.
fn dedupe(pts: Vec<Vec3>) -> Vec<Vec3> {
    let mut seen = std::collections::HashSet::new();
    pts.into_iter().filter(|p| seen.insert(((p.x * 100.0).round() as i32, (p.y * 100.0).round() as i32, (p.z * 100.0).round() as i32))).collect()
}

/// Collision shapes derived from the visual model so the two can never drift
/// apart: the static body is the convex hull of the part's own geometry, a
/// spinning disc / tyre is a cylinder fitted to its sweep.
pub enum ColShape {
    Hull(Vec<Vec3>),
    /// cylinder along local Z
    Cyl { hh: f32, r: f32, c: Vec3 },
}

pub fn collision_shapes(art: &Art, kind: PartKind) -> Vec<ColShape> {
    let mut stat = pts(art, kind, "body");
    for g in ["glow_eye_l", "glow_eye_r"] {
        stat.extend(pts(art, kind, g));
    }
    let stat = dedupe(stat);
    let is_wheel = kind.is_wheel();
    let spin = pts(art, kind, "spin");
    let disc = (!spin.is_empty()).then(|| {
        let mut r: f32 = 0.0;
        let (mut z0, mut z1) = (f32::INFINITY, f32::NEG_INFINITY);
        for q in &spin {
            r = r.max(q.x.hypot(q.y));
            z0 = z0.min(q.z);
            z1 = z1.max(q.z);
        }
        // tyres keep the suspension radius (tread lugs are "soft")
        if is_wheel {
            r = crate::parts::wheel_radius(kind);
        }
        ColShape::Cyl { hh: (z1 - z0) / 2.0, r, c: Vec3::new(0.0, 0.0, (z0 + z1) / 2.0) }
    });
    // order matters to the robot: wheels -> tyre first; spinner -> disc second
    let mut out = vec![];
    let mut disc = disc;
    if is_wheel
        && let Some(d) = disc.take()
    {
        out.push(d);
    }
    if stat.len() >= 4 {
        out.push(ColShape::Hull(stat));
    }
    if let Some(d) = disc {
        out.push(d);
    }
    out
}
