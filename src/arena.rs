//! The battle arena: steel floor, pit, walls, stands full of bobbing crowd,
//! floodlights and flags, instanced from Blender-made pieces (art/arena.py).
use crate::art::Art;
use crate::models::*;
use crate::physics::{grp, groups, Phys};
use bevy::prelude::*;
use rapier3d::prelude::*;
use std::collections::HashSet;
use std::f32::consts::PI;

/// arena half-size
pub const A: f32 = 8.0;
pub struct PitCfg {
    pub x: f32,
    pub z: f32,
    pub h: f32,
}
pub const PIT: PitCfg = PitCfg { x: -5.0, z: 1.2, h: 1.1 };

pub fn floor_y(x: f32, z: f32) -> f32 {
    if (x - PIT.x).abs() < PIT.h && (z - PIT.z).abs() < PIT.h { -2.5 } else { 0.0 }
}
pub fn in_pit(x: f32, z: f32) -> bool {
    (x - PIT.x).abs() < PIT.h + 0.1 && (z - PIT.z).abs() < PIT.h + 0.1
}

struct Person {
    body: Entity,
    head: Entity,
    p: Vec3,
    ry: f32,
    ph: f32,
    amp: f32,
}

#[derive(Component)]
pub struct Crowd {
    people: Vec<Person>,
}

#[derive(Component)]
pub struct PitLight;

pub struct ArenaInfo {
    pub walls: HashSet<ColliderHandle>,
}

/// rect minus hole rect -> up to 4 rects
fn cut_around(r: [f32; 4], h: [f32; 4]) -> Vec<[f32; 4]> {
    let [x0, x1, z0, z1] = r;
    let [hx0, hx1, hz0, hz1] = h;
    if hx1 <= x0 || hx0 >= x1 || hz1 <= z0 || hz0 >= z1 {
        return vec![r];
    }
    let mut out = vec![];
    if hx0 > x0 {
        out.push([x0, hx0, z0, z1]);
    }
    if hx1 < x1 {
        out.push([hx1, x1, z0, z1]);
    }
    let (mx0, mx1) = (x0.max(hx0), x1.min(hx1));
    if hz0 > z0 {
        out.push([mx0, mx1, z0, hz0]);
    }
    if hz1 < z1 {
        out.push([mx0, mx1, hz1, z1]);
    }
    out.retain(|[a, b, c, d]| b - a > 0.02 && d - c > 0.02);
    out
}

pub fn build_arena(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, shared: &Mats, art: &Art, phys: &mut Phys, scope: impl Bundle + Clone) -> ArenaInfo {
    reseed(42);
    let mut walls = HashSet::new();
    let mut ground = |phys: &mut Phys, hx: f32, hy: f32, hz: f32, x: f32, y: f32, z: f32, wall: bool| {
        let h = phys.fixed(
            ColliderBuilder::cuboid(hx, hy, hz)
                .translation(Vec3::new(x, y, z))
                .friction(1.0)
                .friction_combine_rule(CoefficientCombineRule::Multiply)
                .collision_groups(groups(if wall { grp::WALL } else { grp::GROUND }, grp::ALL)),
        );
        if wall {
            walls.insert(h);
        }
    };
    // floor with a hole for the pit
    let (px0, px1, pz0, pz1) = (PIT.x - PIT.h, PIT.x + PIT.h, PIT.z - PIT.h, PIT.z + PIT.h);
    let t = 2.4; // thick, so nothing can punch through the slab
    let e = A + 3.0;
    for [x0, x1, z0, z1] in [[-e, px0, -e, e], [px1, e, -e, e], [px0, px1, -e, pz0], [px0, px1, pz1, e]] {
        ground(phys, (x1 - x0) / 2.0, t / 2.0, (z1 - z0) / 2.0, (x0 + x1) / 2.0, -t / 2.0, (z0 + z1) / 2.0, false);
    }
    // pit bottom + sides
    ground(phys, PIT.h, 0.25, PIT.h, PIT.x, -2.75, PIT.z, false);
    ground(phys, 0.05, 1.3, PIT.h, px0 - 0.05, -1.3, PIT.z, false);
    ground(phys, 0.05, 1.3, PIT.h, px1 + 0.05, -1.3, PIT.z, false);
    ground(phys, PIT.h, 1.3, 0.05, PIT.x, -1.3, pz0 - 0.05, false);
    ground(phys, PIT.h, 1.3, 0.05, PIT.x, -1.3, pz1 + 0.05, false);
    // walls (tall invisible part stops flying bots leaving)
    let wh = 0.6;
    for (x, z, hx, hz) in [(0.0, A + 0.25, A + 0.5, 0.25), (0.0, -A - 0.25, A + 0.5, 0.25), (A + 0.25, 0.0, 0.25, A + 0.5), (-A - 0.25, 0.0, 0.25, A + 0.5)] {
        ground(phys, hx, wh, hz, x, wh, z, true);
        ground(phys, hx, 3.0, hz, x, 3.0 + wh * 2.0, z, true);
    }

    // ------------------------------------------------------------ visuals
    // Blender-made pieces (art/arena.py), instanced along the old layout
    let put = |commands: &mut Commands, meshes: &mut Assets<Mesh>, key: &str, tints: &[Col], t: Transform| {
        commands.spawn((art.place(meshes, key, tints, t), scope.clone()));
    };
    // sandy outer ground: 8.5 m tiles that repeat seamlessly (unrotated)
    for i in -5..5 {
        for j in -5..5 {
            if (-1..=0).contains(&i) && (-1..=0).contains(&j) {
                continue;
            }
            put(commands, meshes, "arena_ground.ground", &[], Transform::from_xyz(i as f32 * 8.5 + 4.25, -0.3, j as f32 * 8.5 + 4.25));
        }
    }
    // dark grout slab under the tiles, cut around the pit
    let mut slab = Geo::new();
    for [x0, x1, z0, z1] in cut_around([-A - 0.2, A + 0.2, -A - 0.2, A + 0.2], [px0, px1, pz0, pz1]) {
        slab.at(&bx(x1 - x0, 0.4, z1 - z0, hexc("#3b3a42"), 0.0, 0.0), (x0 + x1) / 2.0, -0.45, (z0 + z1) / 2.0);
    }
    commands.spawn((Mesh3d(meshes.add(slab.to_mesh())), MeshMaterial3d(shared.vc.clone()), scope.clone()));
    // floor plates: 2 m grid; plates touching the pit shrink to its edge
    let gap = 0.03;
    for i in -4..4 {
        for j in -4..4 {
            let (cx, cz) = (i as f32 * 2.0 + 1.0, j as f32 * 2.0 + 1.0);
            let variant = if rand() < 0.15 { "c" } else if rand() < 0.5 { "a" } else { "b" };
            let turn = (rand() * 4.0).floor() * HALF_PI;
            let r = [(cx - 1.0).max(-A), (cx + 1.0).min(A), (cz - 1.0).max(-A), (cz + 1.0).min(A)];
            let pieces = cut_around(r, [px0 - 0.3, px1 + 0.3, pz0 - 0.3, pz1 + 0.3]);
            let clipped = pieces.len() != 1 || pieces[0] != r;
            for [a, b, d, e] in pieces {
                let (a2, b2, d2, e2) = (a + gap, b - gap, d + gap, e - gap);
                if b2 - a2 < 0.05 || e2 - d2 < 0.05 {
                    continue;
                }
                let t = Transform {
                    translation: Vec3::new((a2 + b2) / 2.0, 0.0, (d2 + e2) / 2.0),
                    rotation: if clipped { Quat::IDENTITY } else { ry(turn) },
                    scale: Vec3::new((b2 - a2) / 1.97, 1.0, (e2 - d2) / 1.97),
                };
                put(commands, meshes, &format!("arena_tile.{variant}"), &[], t);
            }
        }
    }
    // starting squares + centre ring sit where they were modelled
    for g in ["start_red", "start_blue", "ring"] {
        commands.spawn((art.bundle(meshes, &format!("arena_marks.{g}"), &[], Transform::IDENTITY), scope.clone()));
    }
    // pit: hazard border + shaft, with a red glow at the bottom
    put(commands, meshes, "arena_pit.pit", &[], Transform::from_xyz(PIT.x, 0.0, PIT.z));
    commands.spawn((
        Mesh3d(meshes.add(uniform_layout(Plane3d::default().mesh().size(PIT.h * 2.0, PIT.h * 2.0).build()))),
        MeshMaterial3d(glow_mat(mats, hexc("#5a1208"), 2.4)),
        Transform::from_xyz(PIT.x, -2.45, PIT.z),
        scope.clone(),
    ));
    commands.spawn((
        PointLight { color: Color::srgb_u8(0xff, 0x3a, 0x1a), intensity: 60_000.0, range: 5.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(PIT.x, -1.6, PIT.z),
        PitLight,
        scope.clone(),
    ));

    // perimeter walls: 2 m segments, a post every 2 m, corner blocks; a side
    // rotated by t faces the arena with its local -Z
    for t in [0.0, PI, HALF_PI, -HALF_PI] {
        let q = ry(t);
        for k in 0..8 {
            put(commands, meshes, "arena_wall.seg", &[], Transform::from_translation(q * Vec3::new(-7.0 + k as f32 * 2.0, 0.0, A + 0.25)).with_rotation(q));
        }
        for k in 0..=8 {
            put(commands, meshes, "arena_wall.post", &[], Transform::from_translation(q * Vec3::new(-A + k as f32 * 2.0, 0.0, A + 0.25)).with_rotation(q));
        }
        put(commands, meshes, "arena_wall.corner", &[], Transform::from_translation(q * Vec3::new(A + 0.25, 0.0, A + 0.25)).with_rotation(q));
    }

    // stands + crowd (camera side, -Z, stays open)
    let shirts = ["#d8453a", "#3f7fd8", "#f2b632", "#3fa7a3", "#7c5bb5", "#e8e0d0", "#5a8a3a", "#ee7a32", "#2e2d33"];
    let skins = ["#f1c7a1", "#d9a07a", "#a8714a", "#7a4a2e", "#f5d7bd"];
    let mut people = vec![];
    for side in [0usize, 2, 3] {
        let r = [0.0, PI, -HALF_PI, HALF_PI][side];
        let q = ry(r);
        for tier in 0..5 {
            let dist = A + 1.4 + tier as f32 * 0.9;
            let h = 0.5 + tier as f32 * 0.55;
            for k in 0..7 {
                let x = -12.0 + k as f32 * 4.0;
                put(commands, meshes, "arena_stand.step", &[], Transform::from_translation(q * Vec3::new(x, h, dist)).with_rotation(q));
                put(commands, meshes, "arena_stand.fill", &[], Transform { translation: q * Vec3::new(x, h - 0.6, dist), rotation: q, scale: Vec3::new(1.0, h - 0.3, 1.0) });
            }
            let mut k = -A - 5.0;
            while k <= A + 5.0 {
                k += 0.62;
                if rand() < 0.12 {
                    continue;
                }
                let p = q * Vec3::new(k + (rand() - 0.5) * 0.2, h + 0.27, dist + (rand() - 0.5) * 0.1);
                let ph = rand() * 10.0;
                let amp = 0.5 + rand();
                let shirt = hexc(shirts[(rand() * shirts.len() as f32) as usize % shirts.len()]);
                let skin = hexc(skins[(rand() * skins.len() as f32) as usize % skins.len()]);
                let tints = [shirt, skin];
                let body = commands.spawn((art.place(meshes, "crowd_person.body", &tints, Transform::from_translation(p)), scope.clone())).id();
                let head = commands.spawn((art.place(meshes, "crowd_person.head", &tints, Transform::from_translation(p + Vec3::Y * 0.42)), scope.clone())).id();
                people.push(Person { body, head, p, ry: r + PI, ph, amp });
            }
        }
    }

    // floodlight towers, lamps bloom
    let lamp = glow_mat(mats, hexc("#fff6dc"), 2.4);
    for (x, z) in [(-A - 3.0, -A - 3.0), (A + 3.0, -A - 3.0), (-A - 3.0, A + 3.0), (A + 3.0, A + 3.0)] {
        let look = Transform::from_xyz(x, 0.0, z).looking_at(Vec3::ZERO, Vec3::Y) * Transform::from_rotation(ry(PI));
        put(commands, meshes, "arena_tower.tower", &[], look);
        for i in 0..6 {
            commands.spawn((art.glow(&format!("arena_tower.glow_lamp{i}"), lamp.clone(), look), scope.clone()));
        }
    }

    // banners on the back stands
    for i in -3i32..=3 {
        let bg = hexc(["#d8453a", "#3f7fd8", "#f2b632"][((i % 3) + 3) as usize % 3]);
        let fg = if i % 2 != 0 { Pal::ink() } else { Pal::cream() };
        let t = Transform::from_xyz(i as f32 * 3.2, 5.6, A + 6.2);
        for g in ["cloth", "rod"] {
            commands.spawn((art.bundle(meshes, &format!("arena_banner.{g}"), &[bg, fg], t), scope.clone()));
        }
    }
    // stadium back and side walls
    for i in 0..12 {
        put(commands, meshes, "arena_backwall.seg", &[], Transform::from_xyz(-13.75 + i as f32 * 2.5, 0.0, A + 6.6));
    }
    for (sx, t) in [(1.0, HALF_PI), (-1.0, -HALF_PI)] {
        for i in 0..5 {
            put(commands, meshes, "arena_backwall.seg", &[], Transform { translation: Vec3::new(sx * (A + 6.6), 0.0, -4.5 + (i as f32 + 0.5) * 2.6), rotation: ry(t), scale: Vec3::new(1.04, 1.0, 1.0) });
        }
    }
    // the camera side: a low hazard barrier so the foreground isn't empty
    for i in 0..14 {
        put(commands, meshes, "arena_barrier.seg", &[], Transform::from_xyz(-13.0 + i as f32 * 2.0, 0.0, -A - 2.5).with_rotation(ry(PI)));
    }
    // cones
    for _ in 0..6 {
        let mut px = (rand() - 0.5) * 2.0 * (A + 4.0);
        let mut pz = -A - 1.6 + rand() * 0.3;
        if px.abs() < A + 0.7 {
            pz = -A - 1.4;
        }
        if px.abs() < 0.1 {
            px = 0.1;
        }
        put(commands, meshes, "arena_cone.cone", &[], Transform::from_xyz(px, 0.0, pz).with_rotation(ry(rand() * 6.0)));
    }
    commands.spawn((Name::new("Crowd"), Crowd { people }, scope));
    ArenaInfo { walls }
}

/// Bob the crowd and pulse the pit light; `excitement` 0..~2.
pub fn update_arena(tfs: &mut Query<&mut Transform>, crowd: &Crowd, light: &mut PointLight, t: f32, excitement: f32) {
    for p in &crowd.people {
        let b = ((t * (6.0 + p.amp * 3.0) + p.ph).sin()).max(0.0) * (0.03 + excitement * 0.22 * p.amp);
        let rot = Quat::from_rotation_y(p.ry) * Quat::from_rotation_z((t * 2.0 + p.ph).sin() * 0.05 * excitement);
        if let Ok(mut tf) = tfs.get_mut(p.body) {
            *tf = Transform { translation: p.p + Vec3::Y * b, rotation: rot, ..default() };
        }
        if let Ok(mut tf) = tfs.get_mut(p.head) {
            *tf = Transform { translation: p.p + Vec3::Y * (b + 0.42), rotation: rot, ..default() };
        }
    }
    light.intensity = 60_000.0 + (t * 6.0).sin() * 20_000.0;
}
