//! The battle arena: steel floor, pit, walls, stands full of bobbing crowd,
//! floodlights and flags. All static scenery is merged into one mesh.
use crate::models::*;
use crate::physics::{grp, groups, Phys};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
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
    p: Vec3,
    ry: f32,
    ph: f32,
    amp: f32,
    shirt: Col,
    skin: Col,
}

#[derive(Component)]
pub struct Crowd {
    people: Vec<Person>,
    body: Geo,
    head: Geo,
    mesh: Handle<Mesh>,
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

/// A box with diagonal hazard stripes on its long sides (along X), length `w`.
pub fn hazard_box(w: f32, h: f32, d: f32, stripes: usize) -> Geo {
    let (a, b) = (Pal::yellow(), Pal::ink());
    let mut g = Geo::new();
    let seg = w / stripes as f32;
    for i in 0..stripes {
        let x = -w / 2.0 + (i as f32 + 0.5) * seg;
        // each stripe cell: a yellow block with a dark slanted band
        g.at(&bx(seg, h, d, a, 0.0, 0.04), x, 0.0, 0.0);
        let band = bx(seg * 0.42, h * 1.25, d + 0.006, b, 0.0, 0.03);
        let mut cut = band.transformed(Transform { translation: Vec3::new(x, 0.0, 0.0), rotation: Quat::from_rotation_z(0.6), ..default() });
        // clip the slanted band to the box height by squashing it vertically
        for p in cut.pos.iter_mut() {
            p.y = p.y.clamp(-h / 2.0 - 0.002, h / 2.0 + 0.002);
        }
        g.add0(&cut);
    }
    g
}

pub fn build_arena(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, shared: &Mats, phys: &mut Phys, scope: impl Bundle + Clone) -> ArenaInfo {
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
    let mut stat = Geo::new();
    // outer ground
    for [x0, x1, z0, z1] in cut_around([-40.0, 40.0, -40.0, 40.0], [-A - 0.5, A + 0.5, -A - 0.5, A + 0.5]) {
        stat.at(&bx(x1 - x0, 1.0, z1 - z0, hexc("#bfa27a"), 0.0, 0.03), (x0 + x1) / 2.0, -0.8, (z0 + z1) / 2.0);
    }
    // grout slab under the tiles, cut around the pit
    for [x0, x1, z0, z1] in cut_around([-A - 0.2, A + 0.2, -A - 0.2, A + 0.2], [px0, px1, pz0, pz1]) {
        stat.at(&bx(x1 - x0, 0.4, z1 - z0, hexc("#3b3a42"), 0.0, 0.0), (x0 + x1) / 2.0, -0.45, (z0 + z1) / 2.0);
    }
    // floor tiles: 2 m grid, tiles that touch the pit are clipped to its edge
    let gap = 0.03;
    for i in -4..4 {
        for j in -4..4 {
            let (cx, cz) = (i as f32 * 2.0 + 1.0, j as f32 * 2.0 + 1.0);
            // dusty steel plates: warm grey, the odd rust-stained one
            let g = 0.42 + rand() * 0.05;
            let mut c = Vec3::new(g, g * 0.93, g * 0.84);
            if rand() < 0.15 {
                c = c.lerp(Pal::rust(), 0.12 + rand() * 0.12);
            }
            // vertex colours are linear; the prototype's tile values were sRGB-ish
            let c = Vec3::new(c.x.powf(2.2), c.y.powf(2.2), c.z.powf(2.2)) * 1.6;
            let r = [(cx - 1.0).max(-A), (cx + 1.0).min(A), (cz - 1.0).max(-A), (cz + 1.0).min(A)];
            for [a, b, d, e] in cut_around(r, [px0 - 0.3, px1 + 0.3, pz0 - 0.3, pz1 + 0.3]) {
                let (a2, b2, d2, e2) = (a + gap, b - gap, d + gap, e - gap);
                if b2 - a2 < 0.05 || e2 - d2 < 0.05 {
                    continue;
                }
                stat.at(&cbox_j(b2 - a2, 0.08, e2 - d2, c, 0.012, 0.0, 0.03), (a2 + b2) / 2.0, -0.04, (d2 + e2) / 2.0);
            }
        }
    }
    // starting squares + centre ring (thin raised slabs, no z-fighting)
    stat.at(&bx(2.2, 0.01, 2.2, hexc("#b0553c"), 0.0, 0.03), 0.0, 0.005, -5.2);
    stat.at(&bx(2.2, 0.01, 2.2, hexc("#4f78b8"), 0.0, 0.03), 0.0, 0.005, 5.2);
    let segs = 24;
    for k in 0..segs {
        let a0 = k as f32 / segs as f32 * std::f32::consts::TAU;
        let mid = a0 + std::f32::consts::TAU / segs as f32 / 2.0;
        let arc = 1.8 * std::f32::consts::TAU / segs as f32;
        stat.add(&bx(0.4, 0.008, arc + 0.02, Pal::yellow(), 0.0, 0.03), trr(mid.cos() * 1.8, 0.004, mid.sin() * 1.8, ry(-mid)));
    }

    // pit: hazard border + dark open shaft + red glow
    let border = PIT.h * 2.0 + 0.6;
    stat.at(&hazard_box(border, 0.12, 0.3, 8), PIT.x, -0.01, pz0 - 0.15);
    stat.at(&hazard_box(border, 0.12, 0.3, 8), PIT.x, -0.01, pz1 + 0.15);
    stat.add(&hazard_box(PIT.h * 2.0, 0.12, 0.3, 6), trr(px0 - 0.15, -0.01, PIT.z, ry(HALF_PI)));
    stat.add(&hazard_box(PIT.h * 2.0, 0.12, 0.3, 6), trr(px1 + 0.15, -0.01, PIT.z, ry(HALF_PI)));
    let shaft = hexc("#221c24");
    stat.at(&bx(PIT.h * 2.0, 2.5, 0.02, shaft, 0.0, 0.0), PIT.x, -1.27, pz0 - 0.01);
    stat.at(&bx(PIT.h * 2.0, 2.5, 0.02, shaft, 0.0, 0.0), PIT.x, -1.27, pz1 + 0.01);
    stat.at(&bx(0.02, 2.5, PIT.h * 2.0, shaft, 0.0, 0.0), px0 - 0.01, -1.27, PIT.z);
    stat.at(&bx(0.02, 2.5, PIT.h * 2.0, shaft, 0.0, 0.0), px1 + 0.01, -1.27, PIT.z);
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(PIT.h * 2.0, PIT.h * 2.0))),
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

    // walls with a hazard band, posts and a rail
    for (x, z, r) in [(0.0, A + 0.25, 0.0), (0.0, -A - 0.25, 0.0), (A + 0.25, 0.0, HALF_PI), (-A - 0.25, 0.0, HALF_PI)] {
        let mut w = bx(2.0 * A + 1.0, 1.2, 0.5, hexc("#6d6b74"), 0.15, 0.05);
        w.at(&hazard_box(2.0 * A + 1.02, 0.32, 0.52, 24), 0.0, 0.42, 0.0);
        let mut i = -A;
        while i <= A {
            w.at(&bx(0.12, 1.6, 0.12, Pal::steel_dark(), 0.2, 0.07), i, 1.3, 0.0);
            i += 2.0;
        }
        w.at(&bx(2.0 * A + 0.2, 0.08, 0.08, Pal::steel_dark(), 0.2, 0.07), 0.0, 2.06, 0.0);
        stat.add(&w, trr(x, 0.6, z, ry(r)));
    }

    // stands + crowd (camera side, -Z, stays open)
    let mut people = vec![];
    let shirts = ["#d8453a", "#3f7fd8", "#f2b632", "#3fa7a3", "#7c5bb5", "#e8e0d0", "#5a8a3a", "#ee7a32", "#2e2d33"];
    let skins = ["#f1c7a1", "#d9a07a", "#a8714a", "#7a4a2e", "#f5d7bd"];
    for side in [0usize, 2, 3] {
        let r = [0.0, PI, -HALF_PI, HALF_PI][side];
        let q = ry(r);
        for tier in 0..5 {
            let dist = A + 1.4 + tier as f32 * 0.9;
            let h = 0.5 + tier as f32 * 0.55;
            let p = q * Vec3::new(0.0, (h + 0.1) / 2.0 - 0.1, dist);
            stat.add(&bx(2.0 * (A + 6.0), h + 0.1, 0.9, hexc("#6a5f6e"), 0.05, 0.03), trr(p.x, p.y, p.z, q));
            let mut k = -A - 5.0;
            while k <= A + 5.0 {
                k += 0.62;
                if rand() < 0.12 {
                    continue;
                }
                let pp = q * Vec3::new(k + (rand() - 0.5) * 0.2, h + 0.27, dist + (rand() - 0.5) * 0.1);
                people.push(Person {
                    p: pp,
                    ry: r + PI,
                    ph: rand() * 10.0,
                    amp: 0.5 + rand(),
                    shirt: hexc(shirts[(rand() * shirts.len() as f32) as usize % shirts.len()]),
                    skin: hexc(skins[(rand() * skins.len() as f32) as usize % skins.len()]),
                });
            }
        }
    }

    // floodlight towers
    let light = hexc("#fff6dc");
    let mut lamps = Geo::new();
    for (x, z) in [(-A - 3.0, -A - 3.0), (A + 3.0, -A - 3.0), (-A - 3.0, A + 3.0), (A + 3.0, A + 3.0)] {
        let mut g = Geo::new();
        for (dx, dz) in [(-0.35, -0.35), (0.35, -0.35), (-0.35, 0.35), (0.35, 0.35)] {
            g.at(&bx(0.1, 9.0, 0.1, Pal::steel_dark(), 0.2, 0.07), dx, 4.5, dz);
        }
        let mut y = 1.0;
        while y < 9.0 {
            for zz in [-0.35, 0.35] {
                g.add(&bx(0.8, 0.06, 0.06, Pal::steel_dark(), 0.2, 0.07), trr(0.0, y, zz, rz(0.6)));
            }
            y += 1.2;
        }
        let panel_q = rx(-0.35);
        g.add(&bx(2.2, 1.5, 0.3, Pal::ink(), 0.1, 0.07), trr(0.0, 9.4, 0.0, panel_q));
        let mut l = Geo::new();
        for i in 0..3 {
            for j in 0..2 {
                l.add(&cyl(0.28, 0.28, 0.02, 10, light, 0.0), Transform { translation: panel_q * Vec3::new(-0.7 + i as f32 * 0.7, -0.35 + j as f32 * 0.7, 0.16) + Vec3::new(0.0, 9.4, 0.0), rotation: panel_q * rx(HALF_PI), ..default() });
            }
        }
        let look = Transform::from_xyz(x, 0.0, z).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y) * Transform::from_rotation(ry(PI));
        stat.add(&g, look);
        lamps.add(&l, look);
    }
    commands.spawn((Mesh3d(meshes.add(lamps.to_mesh())), MeshMaterial3d(glow_mat(mats, light, 2.4)), bevy::light::NotShadowCaster, scope.clone()));

    // banners on the back stands
    for i in -3i32..=3 {
        let c = hexc(["#d8453a", "#3f7fd8", "#f2b632"][((i % 3) + 3) as usize % 3]);
        let fg = if i % 2 != 0 { Pal::ink() } else { Pal::cream() };
        let mut b = bx(1.4, 2.1, 0.02, c, 0.0, 0.04);
        b.add(&bx(0.18, 1.3, 0.025, fg, 0.0, 0.0), Transform::from_rotation(rz(0.7)));
        b.add(&bx(0.18, 1.3, 0.025, fg, 0.0, 0.0), Transform::from_rotation(rz(-0.7)));
        stat.at(&b, i as f32 * 3.2, 5.6, A + 6.2);
        stat.at(&bx(1.6, 0.08, 0.08, Pal::steel_dark(), 0.12, 0.07), i as f32 * 3.2, 6.68, A + 6.2);
    }
    // back wall of the stadium
    stat.at(&bx(2.0 * (A + 7.0), 7.0, 0.5, hexc("#7a6670"), 0.08, 0.05), 0.0, 3.0, A + 6.6);
    for sx in [-1.0, 1.0] {
        stat.at(&bx(0.5, 7.0, 13.0, hexc("#7a6670"), 0.08, 0.05), sx * (A + 6.6), 3.0, 2.0);
    }
    // the camera-side: a low barrier with hazard stripes so the foreground isn't empty
    stat.at(&bx(2.0 * (A + 6.0), 0.8, 0.6, hexc("#5a5660"), 0.1, 0.05), 0.0, 0.4, -A - 2.5);
    stat.at(&hazard_box(2.0 * (A + 6.0), 0.25, 0.62, 28), 0.0, 0.7, -A - 2.5);
    // cones
    for _ in 0..6 {
        let mut cone = cyl(0.0, 0.25, 0.7, 7, hexc("#ee7a32"), 0.0);
        cone.at(&cyl(0.17, 0.2, 0.12, 7, hexc("#fff3dc"), 0.0), 0.0, 0.05, 0.0);
        let mut px = (rand() - 0.5) * 2.0 * (A + 4.0);
        let mut pz = -A - 1.6 + rand() * 0.3;
        if px.abs() < A + 0.7 {
            pz = -A - 1.4;
        }
        if px.abs() < 0.1 {
            px = 0.1;
        }
        stat.at(&cone, px, 0.35, pz);
    }
    commands.spawn((Mesh3d(meshes.add(stat.to_mesh())), MeshMaterial3d(shared.vc.clone()), scope.clone()));

    // crowd: one mesh rebuilt each frame
    let body = bx(0.42, 0.55, 0.3, Vec3::ONE, 0.0, 0.05);
    let head = bx(0.28, 0.28, 0.28, Vec3::ONE, 0.0, 0.05);
    let mesh = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()));
    let mut crowd = Crowd { people, body, head, mesh: mesh.clone() };
    write_crowd(&mut crowd, meshes, 0.0, 0.0);
    commands.spawn((Mesh3d(mesh), MeshMaterial3d(shared.vc.clone()), bevy::camera::visibility::NoFrustumCulling, crowd, scope));
    ArenaInfo { walls }
}

fn write_crowd(c: &mut Crowd, meshes: &mut Assets<Mesh>, t: f32, excitement: f32) {
    let mut g = Geo::new();
    for p in &c.people {
        let b = ((t * (6.0 + p.amp * 3.0) + p.ph).sin()).max(0.0) * (0.03 + excitement * 0.22 * p.amp);
        let rot = Quat::from_rotation_y(p.ry) * Quat::from_rotation_z((t * 2.0 + p.ph).sin() * 0.05 * excitement);
        let n0 = g.pos.len();
        g.add(&c.body, Transform { translation: p.p + Vec3::Y * b, rotation: rot, ..default() });
        for col in &mut g.col[n0..] {
            *col *= p.shirt;
        }
        let n1 = g.pos.len();
        g.add(&c.head, Transform { translation: p.p + Vec3::Y * (b + 0.42), rotation: rot, ..default() });
        for col in &mut g.col[n1..] {
            *col *= p.skin;
        }
    }
    if let Some(mut m) = meshes.get_mut(&c.mesh) {
        *m = g.to_mesh();
    }
}

/// Bob the crowd and pulse the pit light; `excitement` 0..~2.
pub fn update_arena(meshes: &mut Assets<Mesh>, crowd: &mut Crowd, light: &mut PointLight, t: f32, excitement: f32) {
    write_crowd(crowd, meshes, t, excitement);
    light.intensity = 60_000.0 + (t * 6.0).sin() * 20_000.0;
}
