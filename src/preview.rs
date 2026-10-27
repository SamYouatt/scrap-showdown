//! Static (no physics) robot model for the workshop, title screen, etc.
use crate::art::Art;
use crate::partmodels::{spawn_part, PartLook};
use crate::parts::{ground_level, Design, Placed, LEG_LEN};
use crate::robot::{EyeMats, Eyes};
use bevy::prelude::*;

/// Root of a spawned preview; `ride` is how high the core sits off the floor.
#[derive(Component, Clone, Copy)]
pub struct PreviewRoot {
    pub ride: f32,
}

/// Leg segments (upper, lower, foot) posed standing on the floor, in the
/// part's local space.
pub fn static_leg(p: &Placed) -> [Transform; 3] {
    let q = p.rot;
    let inv = q.inverse();
    let hip = Vec3::new(0.0, 0.0, 0.19);
    // splay the way the leg points (same rule as the walking robot)
    let mut out_w = q * Vec3::Z;
    out_w.y = 0.0;
    let sprawl = (out_w.length() * 1.4).min(1.0);
    let outward = inv * (out_w.normalize_or_zero() * 0.3 * sprawl);
    let down = inv * Vec3::new(0.0, -LEG_LEN, 0.0);
    let foot_p = hip + down + outward;
    let up = inv * Vec3::Y;
    let back = inv * Vec3::NEG_Z;
    let knee = hip.lerp(foot_p, 0.5) + up * 0.12 + outward * 1.2 + back * 0.2 * (1.0 - sprawl);
    let look = |from: Vec3, to: Vec3| Quat::from_rotation_arc(Vec3::Z, (to - from).normalize_or(Vec3::Z));
    [
        Transform { translation: hip, rotation: look(hip, knee), ..default() },
        Transform { translation: knee, rotation: look(knee, foot_p), ..default() },
        Transform { translation: foot_p, rotation: inv, ..default() },
    ]
}

/// Spawn one part at rest (moving bits at their rest pose, legs standing) at
/// `t`; returns the part's entity.
pub fn spawn_static_part(commands: &mut Commands, meshes: &mut Assets<Mesh>, art: &Art, pl: &Placed, look: &PartLook, t: Transform, extra: impl Bundle + Clone) -> Entity {
    let pe = spawn_part(commands, meshes, art, pl.kind, look, t, extra);
    let rest = match pl.kind {
        crate::parts::PartKind::Hammer => crate::parts::HAMMER.rest,
        _ => crate::parts::FLIP.rest,
    };
    if let Some((e, pivot)) = pe.arm {
        commands.entity(e).insert(Transform { translation: pivot, rotation: Quat::from_rotation_x(-rest), ..default() });
    }
    if let Some(legs) = pe.legs {
        for (e, tf) in legs.into_iter().zip(static_leg(pl)) {
            commands.entity(e).insert(tf);
            commands.entity(pe.ent).add_child(e);
        }
    }
    pe.ent
}

pub fn part_look(paint: Color, eyes: &EyeMats) -> PartLook {
    PartLook { paint: crate::models::col_of(paint), worn: false, ghost: None, eye: eyes.get(Eyes::Calm), tip: eyes.tip() }
}

/// Spawn a static bot. Returns the root entity.
pub fn spawn_preview(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    art: &Art,
    eyes: &EyeMats,
    d: &Design,
    show_damage: bool,
    transform: Transform,
    extra: impl Bundle + Clone,
) -> Entity {
    let ride = -ground_level(d);
    let root = commands.spawn((Name::new(d.name.clone()), transform, Visibility::default(), PreviewRoot { ride }, extra.clone())).id();
    for pl in &d.parts {
        let mut look = part_look(d.paint, eyes);
        let mut rot = pl.rot;
        if show_damage && pl.dmg > 0.45 {
            look.worn = true;
            rot *= Quat::from_rotation_z(pl.dmg * 0.15 * if pl.id % 2 == 1 { 1.0 } else { -1.0 });
        }
        let child = spawn_static_part(commands, meshes, art, pl, &look, Transform { translation: pl.pos, rotation: rot, ..default() }, extra.clone());
        commands.entity(root).add_child(child);
    }
    root
}
