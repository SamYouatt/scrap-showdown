//! Static (no physics) robot model for the workshop, title screen, etc.
use crate::models::{self, Geo, Mats};
use crate::partmodels::build_part;
use crate::parts::{ground_level, Design, Placed, LEG_LEN};
use bevy::prelude::*;

/// Marks one part's entity inside a preview so the workshop can pick it.
#[derive(Component, Clone, Copy)]
pub struct PreviewPart {
    pub id: u32,
}

/// Root of a spawned preview; `ride` is how high the core sits off the floor.
#[derive(Component, Clone, Copy)]
pub struct PreviewRoot {
    pub ride: f32,
}

/// Leg segments posed standing on the floor, in the part's local space.
pub fn static_leg(p: &Placed, upper: &Geo, lower: &Geo, foot: &Geo) -> Geo {
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
    let mut g = Geo::new();
    g.add(upper, Transform { translation: hip, rotation: look(hip, knee), ..default() });
    g.add(lower, Transform { translation: knee, rotation: look(knee, foot_p), ..default() });
    g.add(foot, Transform { translation: foot_p, rotation: inv, ..default() });
    g
}

/// One part's merged mesh at rest (moving bits baked in, legs standing), in
/// part-local space, plus the part model for its glowing bits.
pub fn part_geo(pl: &Placed, paint: models::Col) -> (Geo, crate::partmodels::PartModel) {
    let m = build_part(pl.kind, paint);
    let mut g = m.body.clone();
    if let Some((s, o)) = &m.spin {
        g.add(s, Transform::from_translation(*o));
    }
    if let Some((a, o)) = &m.arm {
        let rest = match pl.kind {
            crate::parts::PartKind::Hammer => crate::parts::HAMMER.rest,
            _ => crate::parts::FLIP.rest,
        };
        g.add(a, Transform { translation: *o, rotation: Quat::from_rotation_x(-rest), ..default() });
    }
    if let Some(w) = &m.whip {
        g.add(&w.rod, Transform::from_translation(w.origin));
        g.add(&models::ico(0.035, w.tip_col * 1.5), Transform::from_translation(w.origin + w.tip));
    }
    if let Some(l) = &m.leg {
        g.add0(&static_leg(pl, &l.upper, &l.lower, &l.foot));
    }
    (g, m)
}

/// Spawn a static bot. Each part is one merged mesh (moving bits baked in at
/// their rest pose). Returns the root entity.
pub fn spawn_preview(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    shared: &Mats,
    d: &Design,
    show_damage: bool,
    transform: Transform,
    extra: impl Bundle + Clone,
) -> Entity {
    let ride = -ground_level(d);
    let paint = models::col_of(d.paint);
    let root = commands.spawn((transform, Visibility::default(), PreviewRoot { ride }, extra.clone())).id();
    for pl in &d.parts {
        let (mut g, m) = part_geo(pl, paint);
        let mut rot = pl.rot;
        if show_damage && pl.dmg > 0.45 {
            g.tint(Vec3::new(0.62, 0.57, 0.54));
            rot *= Quat::from_rotation_z(pl.dmg * 0.15 * if pl.id % 2 == 1 { 1.0 } else { -1.0 });
        }
        let child = commands
            .spawn((
                Mesh3d(meshes.add(g.to_mesh())),
                MeshMaterial3d(shared.vc.clone()),
                Transform { translation: pl.pos, rotation: rot, ..default() },
                PreviewPart { id: pl.id },
                extra.clone(),
            ))
            .with_children(|c| {
                for (eye, t, col) in &m.glow {
                    c.spawn((Mesh3d(meshes.add(eye.to_mesh())), MeshMaterial3d(models::glow_mat(mats, *col, 3.0)), *t, extra.clone()));
                }
            })
            .id();
        commands.entity(root).add_child(child);
    }
    root
}
