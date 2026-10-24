//! Detailed low-poly part models. Part-local convention (see parts.rs):
//! the attach face is at z = 0 and the part extends along +Z; +Y is the part's
//! "up" arrow (the direction a hammer swings / a flipper plate points).
use crate::models::*;
use crate::parts::{PartKind, FLIP, HAMMER, WEDGE_PROFILE};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

/// A thin springy antenna: bends instead of going through the floor.
#[derive(Clone)]
pub struct Whip {
    pub origin: Vec3,
    pub rod: Geo,
    pub tip: Vec3,
    pub tip_col: Col,
}

#[derive(Clone)]
pub struct LegModel {
    pub upper: Geo,
    pub lower: Geo,
    pub foot: Geo,
}

#[derive(Clone, Default)]
pub struct PartModel {
    /// everything rigidly attached to the part, in part-local space
    pub body: Geo,
    /// wheel / disc: spins about local Z, origin in part-local space
    pub spin: Option<(Geo, Vec3)>,
    /// hammer / flipper: rotates about local X at the pivot (rotation.x = -angle)
    pub arm: Option<(Geo, Vec3)>,
    /// glowing bits (eyes): geo at its own origin, where it sits, colour
    pub glow: Vec<(Geo, Transform, Col)>,
    pub whip: Option<Whip>,
    pub leg: Option<LegModel>,
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}
fn orange() -> Col {
    hexc("#c8642e")
}

fn corners(g: &mut Geo, w: f32, d: f32, y: f32, inset: f32, r: f32) {
    for sx in [-1.0, 1.0] {
        for sz in [-1.0, 1.0] {
            g.at(&bolt(r, Pal::steel_dark()), sx * (w / 2.0 - inset), y, sz * (d / 2.0 - inset));
        }
    }
}

/// Bolts round the edge of a face (face in XY plane, facing +Z, at z).
fn face_bolts(g: &mut Geo, w: f32, h: f32, z: f32, inset: f32, r: f32) {
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            g.add(&bolt(r, Pal::steel_dark()), trr(sx * (w / 2.0 - inset), sy * (h / 2.0 - inset), z, rx(HALF_PI)));
        }
    }
}

/// A box with a recessed darker panel + bolts on each of its 4 sides and top.
fn panel_box(w: f32, h: f32, d: f32, col: Col, panel: Col, rust: f32) -> Geo {
    let mut g = cbox(w, h, d, col, 0.035, rust);
    for (ryv, len) in [(0.0, w), (HALF_PI, d), (PI, w), (-HALF_PI, d)] {
        let depth = if ryv == 0.0 || ryv == PI { d } else { w };
        let mut f = Geo::new();
        f.at(&cbox(len * 0.66, h * 0.55, 0.02, panel, 0.008, 0.25), 0.0, 0.0, depth / 2.0);
        face_bolts(&mut f, len * 0.82, h * 0.72, depth / 2.0 + 0.005, 0.0, 0.018);
        g.add(&f, Transform::from_rotation(ry(ryv)));
    }
    g.at(&cbox(w * 0.7, 0.02, d * 0.7, panel, 0.008, 0.3), 0.0, h / 2.0, 0.0);
    corners(&mut g, w * 0.92, d * 0.92, h / 2.0 + 0.01, 0.03, 0.022);
    g
}

/// Build a part's model. Deterministic per kind (reseeds), so the same part
/// always looks the same.
pub fn build_part(kind: PartKind, paint_c: Col) -> PartModel {
    let paint_geo = |p: &Polys, c: Col, r: f32, j: f32| crate::models::paint(p, c, r, j);
    let paint = paint_c;
    reseed(17 + kind as u64 * 101);
    let mut out = PartModel::default();
    let steel = Pal::steel();
    let dark = Pal::steel_dark();
    let ink = Pal::ink();
    let b = kind.def().bounds;
    let g = &mut out.body;
    match kind {
        PartKind::Core => {
            g.add0(&cbox(0.54, 0.4, 0.54, paint, 0.05, 0.1));
            // contrasting belly band
            g.at(&cbox(0.56, 0.08, 0.56, darker(paint, 0.6), 0.02, 0.2), 0.0, -0.13, 0.0);
            // face plate with eyes
            let mut face = Geo::new();
            face.at(&cbox(0.44, 0.22, 0.05, ink, 0.02, 0.0), 0.0, 0.04, 0.01);
            for sx in [-0.1, 0.1] {
                face.add(&cyl(0.075, 0.075, 0.05, 10, dark, 0.0), trr(sx, 0.04, 0.04, rx(HALF_PI)));
                let eye = ico(0.055, Vec3::ONE).transformed(Transform::from_scale(v(1.0, 1.0, 0.45)));
                out.glow.push((eye, tr(sx, 0.04, 0.27 + 0.065), hexc("#ffd25a")));
            }
            // grille mouth
            vents(&mut face, 0.22, 0.06, 3, dark, tr(0.0, -0.08, 0.03));
            g.at(&face, 0.0, 0.0, 0.27);
            // top hatch with hinges and handle
            g.at(&cbox(0.3, 0.04, 0.28, darker(paint, 0.82), 0.012, 0.2), 0.0, 0.215, -0.04);
            g.at(&cbox(0.12, 0.03, 0.03, dark, 0.008, 0.0), 0.0, 0.245, 0.06);
            for sx in [-0.1, 0.1] {
                g.add(&cyl(0.02, 0.02, 0.06, 6, dark, 0.0), trr(sx, 0.225, -0.185, rz(HALF_PI)));
            }
            corners(g, 0.46, 0.46, 0.205, 0.0, 0.02);
            // side vents
            for sx in [-1.0f32, 1.0] {
                vents(g, 0.22, 0.14, 4, ink, trr(sx * 0.275, 0.02, -0.04, ry(sx * HALF_PI)));
            }
            // antenna base, whip, cable to the back
            g.at(&cyl(0.035, 0.045, 0.05, 8, dark, 0.0), -0.17, 0.225, -0.17);
            out.whip = Some(Whip {
                origin: v(-0.17, 0.24, -0.17),
                rod: rod(Vec3::ZERO, v(0.0, 0.38, -0.02), 0.012, dark, 4, 0.0),
                tip: v(0.0, 0.4, -0.02),
                tip_col: hexc("#ff5a3c"),
            });
            g.add0(&cable(&[v(0.12, 0.2, -0.22), v(0.18, 0.3, -0.3), v(0.22, 0.12, -0.3), v(0.2, 0.0, -0.275)], 0.016, hexc("#2f5fa8")));
        }
        PartKind::Block => {
            g.at(&panel_box(0.5, 0.36, 0.5, steel, dark, 0.18), 0.0, 0.0, b.c.z);
            // weld seam where it meets the parent
            g.at(&cbox(0.42, 0.28, 0.02, dark, 0.005, 0.4), 0.0, 0.0, 0.01);
        }
        PartKind::Slab => {
            let l = 0.9;
            g.at(&cbox(0.5, 0.14, l, hexc("#a2a6ad"), 0.03, 0.2), 0.0, 0.0, l / 2.0);
            // lightening holes
            for i in 0..3 {
                g.at(&cyl(0.075, 0.075, 0.02, 10, ink, 0.0), 0.0, 0.07, 0.18 + i as f32 * 0.27);
            }
            // edge stiffeners + bolts
            for sx in [-1.0, 1.0] {
                g.at(&cbox(0.04, 0.05, l * 0.96, dark, 0.01, 0.3), sx * 0.23, 0.085, l / 2.0);
                bolt_row(g, v(sx * 0.17, 0.075, 0.08), v(sx * 0.17, 0.075, l - 0.08), 5, 0.017, Vec3::Y, dark);
            }
            g.at(&cbox(0.5, 0.06, 0.06, Pal::yellow(), 0.01, 0.1), 0.0, 0.0, l - 0.02);
        }
        PartKind::Girder => {
            let (l, s) = (0.84, 0.075);
            let col = hexc("#d6a24a");
            for sx in [-1.0, 1.0] {
                for sy in [-1.0, 1.0] {
                    g.at(&cbox(0.035, 0.035, l, col, 0.008, 0.25), sx * s, sy * s, l / 2.0);
                }
            }
            // zig-zag lacing on all 4 sides
            let n = 5;
            for i in 0..n {
                let z0 = i as f32 / n as f32 * l;
                let z1 = (i + 1) as f32 / n as f32 * l;
                let flip = if i % 2 == 1 { 1.0 } else { -1.0 };
                for sx in [-1.0, 1.0] {
                    g.add0(&rod(v(sx * s, -s * flip, z0), v(sx * s, s * flip, z1), 0.012, darker(col, 0.85), 4, 0.2));
                }
                for sy in [-1.0, 1.0] {
                    g.add0(&rod(v(-s * flip, sy * s, z0), v(s * flip, sy * s, z1), 0.012, darker(col, 0.85), 4, 0.2));
                }
            }
            for z in [0.02, l - 0.02] {
                g.at(&cbox(0.2, 0.2, 0.04, dark, 0.01, 0.2), 0.0, 0.0, z);
                face_bolts(g, 0.18, 0.18, z + 0.02, 0.02, 0.016);
            }
        }
        PartKind::Armour => {
            g.at(&cbox_j(0.6, 0.48, 0.08, paint, 0.025, 0.2, 0.1), 0.0, 0.0, 0.04);
            // raised centre + hazard stripe
            g.at(&cbox(0.42, 0.3, 0.03, darker(paint, 0.88), 0.01, 0.25), 0.0, 0.02, 0.085);
            for i in 0..4 {
                let c = if i % 2 == 1 { ink } else { Pal::yellow() };
                g.add(&cbox(0.06, 0.09, 0.012, c, 0.002, 0.0), trr(-0.09 + i as f32 * 0.06, -0.19, 0.085, rz(0.5)));
            }
            // rivet border
            for i in 0..6 {
                for sy in [-1.0, 1.0] {
                    g.add(&bolt(0.016, steel), trr(-0.25 + i as f32 * 0.1, sy * 0.2, 0.085, rx(HALF_PI)));
                }
            }
        }
        PartKind::Battery => {
            let mut c = cbox(0.38, 0.28, 0.3, hexc("#33343c"), 0.025, 0.05);
            // label band
            c.at(&cbox(0.385, 0.1, 0.305, Pal::yellow(), 0.01, 0.05), 0.0, 0.03, 0.0);
            c.at(&cbox(0.15, 0.04, 0.307, ink, 0.005, 0.0), -0.07, 0.03, 0.0);
            // terminals on the +Z (outward) face + cables
            for (sx, col) in [(-0.1, Pal::red()), (0.1, ink)] {
                c.add(&cyl(0.035, 0.04, 0.06, 6, col, 0.0), trr(sx, 0.06, 0.17, rx(HALF_PI)));
                c.add0(&cable(&[v(sx, 0.06, 0.2), v(sx * 1.6, 0.13, 0.22), v(sx * 2.0, 0.17, 0.05), v(sx * 1.9, 0.15, -0.15)], 0.014, col));
            }
            // carry handle
            c.add0(&rod(v(-0.12, -0.1, 0.16), v(0.12, -0.1, 0.16), 0.012, ink, 4, 0.0));
            g.at(&c, 0.0, 0.0, b.c.z);
        }
        PartKind::Wheel | PartKind::BigWheel => {
            let big = kind == PartKind::BigWheel;
            let (r, w) = if big { (0.42, 0.3) } else { (0.3, 0.22) };
            let zc = b.c.z + 0.02;
            // bracket + motor between the parent and the wheel
            g.at(&cbox(0.2, 0.2, 0.05, dark, 0.012, 0.25), 0.0, 0.0, 0.025);
            face_bolts(g, 0.18, 0.18, 0.05, 0.02, 0.015);
            g.add(&cyl(0.075, 0.075, zc - w / 2.0 - 0.04, 8, hexc("#4a4d57"), 0.1), trr(0.0, 0.0, (zc - w / 2.0 + 0.04) / 2.0 + 0.01, rx(HALF_PI)));
            for i in 0..3 {
                g.add(&cyl(0.095, 0.095, 0.012, 8, dark, 0.0), trr(0.0, 0.0, 0.08 + i as f32 * 0.03, rx(HALF_PI)));
            }
            let mut spin = Geo::new();
            spin.add(&cyl(r, r, w, if big { 14 } else { 12 }, Pal::tyre(), 0.0), Transform::from_rotation(rx(HALF_PI)));
            // chunky tread blocks, chevrons on the tractor wheel
            let n_tread = if big { 14 } else { 12 };
            let sides: &[f32] = if big { &[-1.0, 1.0] } else { &[0.0] };
            for i in 0..n_tread {
                for &side in sides {
                    let a = (i as f32 + if side > 0.0 { 0.5 } else { 0.0 }) / n_tread as f32 * TAU;
                    let lug = if big { cbox_j(0.12, 0.07, w * 0.5, Pal::tyre(), 0.012, 0.0, 0.05) } else { cbox_j(0.08, 0.05, w * 0.86, Pal::tyre(), 0.012, 0.0, 0.05) };
                    spin.add(&lug, trr(a.cos() * (r + 0.01), a.sin() * (r + 0.01), side * w * 0.25, rz(a - HALF_PI)));
                }
            }
            // rim with spokes, hub, lug nuts
            let rim_c = if big { hexc("#d8453a") } else { Pal::yellow() };
            for sz in [-1.0, 1.0] {
                spin.add(&cyl(r * 0.6, r * 0.6, 0.03, 10, rim_c, 0.2), trr(0.0, 0.0, sz * (w / 2.0 + 0.005), rx(HALF_PI)));
            }
            spin.add(&cyl(r * 0.24, r * 0.28, 0.06, 8, dark, 0.1), trr(0.0, 0.0, w / 2.0 + 0.03, rx(HALF_PI)));
            for i in 0..5 {
                let a = i as f32 / 5.0 * TAU;
                spin.add(&cbox(0.035, r * 0.5, 0.025, darker(rim_c, 0.75), 0.005, 0.1), trr(a.cos() * r * 0.33, a.sin() * r * 0.33, w / 2.0 + 0.02, rz(a - HALF_PI)));
                spin.add(&bolt(0.016, steel), trr((a + 0.6).cos() * r * 0.17, (a + 0.6).sin() * r * 0.17, w / 2.0 + 0.065, rx(HALF_PI)));
            }
            out.spin = Some((spin, v(0.0, 0.0, zc)));
        }
        PartKind::Leg => {
            // hip actuator in the body; the limb is posed by IK in world space
            g.at(&cbox(0.28, 0.3, 0.12, dark, 0.02, 0.2), 0.0, 0.0, 0.06);
            face_bolts(g, 0.26, 0.28, 0.12, 0.03, 0.016);
            g.add(&cyl(0.12, 0.12, 0.18, 10, orange(), 0.25), trr(0.0, 0.0, 0.19, rx(HALF_PI)));
            g.add(&cyl(0.05, 0.05, 0.04, 6, steel, 0.0), trr(0.0, 0.0, 0.29, rx(HALF_PI)));
            let mut upper = Geo::new();
            upper.at(&cbox(0.13, 0.13, 0.44, orange(), 0.03, 0.25), 0.0, 0.0, 0.21);
            upper.add0(&rod(v(0.08, 0.04, 0.05), v(0.08, 0.04, 0.36), 0.025, steel, 6, 0.0));
            upper.at(&cbox(0.06, 0.06, 0.12, dark, 0.01, 0.0), 0.08, 0.04, 0.08);
            let mut lower = Geo::new();
            lower.at(&cbox(0.1, 0.1, 0.42, steel, 0.02, 0.2), 0.0, 0.0, 0.22);
            lower.add(&cyl(0.085, 0.085, 0.17, 10, dark, 0.1), Transform::from_rotation(rz(HALF_PI)));
            lower.add(&cyl(0.04, 0.04, 0.19, 6, Pal::yellow(), 0.0), Transform::from_rotation(rz(HALF_PI)));
            let mut foot = Geo::new();
            foot.at(&cbox(0.24, 0.08, 0.3, dark, 0.02, 0.15), 0.0, -0.02, 0.04);
            for sx in [-0.08, 0.0, 0.08] {
                foot.at(&cbox(0.06, 0.05, 0.08, orange(), 0.01, 0.1), sx, -0.03, 0.22);
            }
            out.leg = Some(LegModel { upper, lower, foot });
        }
        PartKind::Spinner => {
            // motor housing with fins
            g.at(&cbox(0.36, 0.36, 0.14, dark, 0.025, 0.2), 0.0, 0.0, 0.07);
            for i in 0..4 {
                g.at(&cbox(0.38, 0.02, 0.1, ink, 0.004, 0.0), 0.0, -0.12 + i as f32 * 0.08, 0.07);
            }
            g.add(&cyl(0.08, 0.1, 0.06, 8, ink, 0.0), trr(0.0, 0.0, 0.15, rx(HALF_PI)));
            let mut spin = Geo::new();
            spin.add(&cyl(0.42, 0.42, 0.06, 16, hexc("#d9d5cc"), 0.15), Transform::from_rotation(rx(HALF_PI)));
            spin.add(&cyl(0.15, 0.15, 0.09, 8, Pal::red(), 0.1), Transform::from_rotation(rx(HALF_PI)));
            for i in 0..6 {
                let a = i as f32 / 6.0 * TAU;
                spin.add(&bolt(0.018, steel), trr(a.cos() * 0.1, a.sin() * 0.1, 0.05, rx(HALF_PI)));
            }
            for i in 0..3 {
                let a = i as f32 / 3.0 * TAU;
                let tooth = cyl_j(0.0, 0.11, 0.18, 4, hexc("#f0ece2"), 0.05, 0.04);
                spin.add(&tooth, Transform { translation: v(a.cos() * 0.46, a.sin() * 0.46, 0.0), rotation: rz(a - HALF_PI), scale: v(1.0, 1.0, 0.4) });
                // painted danger slot
                spin.add(&cbox(0.14, 0.03, 0.065, Pal::red(), 0.005, 0.0), trr((a + 1.0).cos() * 0.28, (a + 1.0).sin() * 0.28, 0.0, rz(a + 1.0)));
            }
            out.spin = Some((spin, v(0.0, 0.0, 0.2)));
        }
        PartKind::Hammer => {
            g.at(&cbox(0.4, 0.4, 0.12, dark, 0.025, 0.2), 0.0, 0.0, 0.06);
            face_bolts(g, 0.38, 0.38, 0.12, 0.04, 0.017);
            let (py, pz) = (HAMMER.pivot.y, HAMMER.pivot.z);
            for sx in [-0.11, 0.11] {
                g.at(&cbox(0.05, 0.14, pz + 0.02, ink, 0.01, 0.1), sx, py, (pz + 0.02) / 2.0 + 0.05);
            }
            // pneumatic ram
            g.add0(&rod(v(0.0, 0.12, 0.12), v(0.0, py + 0.12, pz + 0.05), 0.04, hexc("#4a4d57"), 8, 0.0));
            g.add0(&rod(v(0.0, 0.12, 0.12), v(0.0, 0.14, 0.24), 0.05, Pal::yellow(), 8, 0.0));
            out.arm = Some((hammer_arm(paint), v(0.0, py, pz)));
        }
        PartKind::Flipper => {
            g.at(&cbox(0.48, 0.44, 0.1, dark, 0.02, 0.2), 0.0, 0.0, 0.05);
            face_bolts(g, 0.46, 0.42, 0.1, 0.04, 0.016);
            // hinge knuckles
            for sx in [-0.18, 0.0, 0.18] {
                g.add(&cyl(0.035, 0.035, 0.1, 8, steel, 0.0), trr(sx, FLIP.hinge_y, FLIP.hinge_z, rz(HALF_PI)));
            }
            g.add0(&rod(v(0.0, 0.0, 0.08), v(0.0, 0.12, 0.16), 0.05, Pal::yellow(), 8, 0.0));
            let mut arm = Geo::new();
            arm.at(&cbox(0.5, FLIP.len, 0.045, Pal::yellow(), 0.012, 0.12), 0.0, FLIP.len / 2.0, 0.02);
            for i in 0..4 {
                arm.at(&cbox(0.5, 0.06, 0.012, ink, 0.003, 0.0), 0.0, 0.2 + i as f32 * 0.15, 0.047);
            }
            // ribs underneath + lip
            for sx in [-0.18, 0.18] {
                arm.at(&cbox(0.03, FLIP.len * 0.9, 0.05, dark, 0.008, 0.2), sx, FLIP.len * 0.45, -0.02);
            }
            arm.at(&cbox(0.52, 0.05, 0.03, steel, 0.008, 0.1), 0.0, FLIP.len - 0.01, 0.0);
            out.arm = Some((arm, v(0.0, FLIP.hinge_y, FLIP.hinge_z)));
        }
        PartKind::Spike => {
            g.add(&cyl(0.16, 0.18, 0.08, 8, dark, 0.2), trr(0.0, 0.0, 0.04, rx(HALF_PI)));
            for i in 0..6 {
                let a = i as f32 / 6.0 * TAU;
                g.add(&bolt(0.016, dark), trr(a.cos() * 0.13, a.sin() * 0.13, 0.085, rx(HALF_PI)));
            }
            g.add(&cyl(0.12, 0.13, 0.08, 8, steel, 0.1), trr(0.0, 0.0, 0.12, rx(HALF_PI)));
            g.add(&cyl(0.135, 0.135, 0.03, 8, Pal::red(), 0.0), trr(0.0, 0.0, 0.17, rx(HALF_PI)));
            g.add(&cyl_j(0.0, 0.12, 0.56, 8, hexc("#c9ccd2"), 0.05, 0.05), trr(0.0, 0.0, 0.44, rx(HALF_PI)));
            for i in 0..4 {
                let a = i as f32 / 4.0 * TAU + PI / 4.0;
                g.add(&cyl(0.0, 0.04, 0.2, 5, steel, 0.05), trr(a.cos() * 0.1, a.sin() * 0.1, 0.26, rx(HALF_PI)));
            }
        }
        PartKind::Wedge => {
            g.add0(&paint_geo(&extrude_zy(&WEDGE_PROFILE, 0.5), hexc("#7c8a46"), 0.25, 0.07));
            // lip strip + ribs + back plate
            let (tz, ty) = WEDGE_PROFILE[2];
            let top_y = WEDGE_PROFILE[1].1;
            g.at(&cbox(0.52, 0.035, 0.06, Pal::yellow(), 0.006, 0.0), 0.0, ty - 0.015, tz - 0.01);
            let slope = (top_y - ty).atan2(tz);
            for sx in [-0.15, 0.15] {
                let rib = cbox(0.03, 0.03, tz.hypot(top_y - ty) * 0.9, dark, 0.005, 0.2);
                g.add(&rib, trr(sx, (top_y + ty) / 2.0 + 0.02, tz / 2.0, rx(slope)));
            }
            g.at(&cbox(0.5, 0.36, 0.03, dark, 0.008, 0.2), 0.0, 0.0, 0.015);
        }
    }
    out
}

/// Hammer arm in its own frame (origin = pivot, shaft along +Z, blade +Y).
fn hammer_arm(paint: Col) -> Geo {
    let steel = Pal::steel();
    let dark = Pal::steel_dark();
    let ink = Pal::ink();
    let mut arm = Geo::new();
    arm.add(&cyl(0.06, 0.06, 0.26, 8, Pal::yellow(), 0.1), Transform::from_rotation(rz(HALF_PI)));
    let l = HAMMER.len;
    arm.at(&cbox(0.08, 0.08, l, steel, 0.015, 0.2), 0.0, 0.0, l / 2.0);
    arm.add0(&rod(v(0.0, -0.07, 0.05), v(0.0, 0.0, l * 0.6), 0.016, dark, 4, 0.0));
    let mut head = cbox(0.24, 0.2, 0.22, paint, 0.03, 0.15);
    head.add0(&cbox(0.27, 0.08, 0.24, ink, 0.01, 0.0));
    // axe blade on the striking side (+Y)
    let blade = cyl_j(0.0, 0.16, 0.24, 4, hexc("#e8e4da"), 0.05, 0.04);
    head.add(&blade, Transform { translation: v(0.0, 0.21, 0.0), rotation: ry(PI / 4.0) * rz(PI), scale: v(1.0, 1.0, 0.35) });
    // hammer face on the back
    head.at(&cyl(0.08, 0.09, 0.07, 8, steel, 0.1), 0.0, -0.13, 0.0);
    arm.at(&head, 0.0, 0.0, l);
    arm
}

/// Hammer arm as two point clouds (shaft, head) for convex hull colliders,
/// in the arm's own frame.
pub fn hammer_arm_hulls() -> (Vec<Vec3>, Vec<Vec3>) {
    reseed(17 + PartKind::Hammer as u64 * 101);
    let g = hammer_arm(Vec3::ONE);
    let (mut shaft, mut head) = (vec![], vec![]);
    for p in &g.pos {
        if p.z > HAMMER.len - 0.16 { head.push(*p) } else { shaft.push(*p) }
    }
    (shaft, head)
}

/// Collision shapes derived from the visual model so the two can never drift
/// apart: the static body is the convex hull of the part's own geometry, a
/// spinning disc / tyre is a cylinder fitted to its sweep.
pub enum ColShape {
    Hull(Vec<Vec3>),
    /// cylinder along local Z
    Cyl { hh: f32, r: f32, c: Vec3 },
}

pub fn collision_shapes(kind: PartKind) -> Vec<ColShape> {
    let m = build_part(kind, Vec3::ONE);
    // dedupe static points
    let mut seen = std::collections::HashSet::new();
    let mut stat = vec![];
    for p in &m.body.pos {
        let key = ((p.x * 200.0).round() as i32, (p.y * 200.0).round() as i32, (p.z * 200.0).round() as i32);
        if seen.insert(key) {
            stat.push(*p);
        }
    }
    for (eye, t, _) in &m.glow {
        stat.extend(eye.pos.iter().map(|p| t.transform_point(*p)));
    }
    let mut out = vec![];
    let is_wheel = kind.is_wheel();
    let disc = m.spin.as_ref().map(|(g, o)| {
        let mut r: f32 = 0.0;
        let (mut z0, mut z1) = (f32::INFINITY, f32::NEG_INFINITY);
        for p in &g.pos {
            let q = *p + *o;
            r = r.max(q.x.hypot(q.y));
            z0 = z0.min(q.z);
            z1 = z1.max(q.z);
        }
        // tyres keep the suspension radius (tread lugs are "soft")
        if is_wheel {
            r = crate::parts::wheel_radius(kind);
        }
        ColShape::Cyl { hh: (z1 - z0) / 2.0, r, c: v(0.0, 0.0, (z0 + z1) / 2.0) }
    });
    // order matters to the robot: wheels -> tyre first; spinner -> disc second
    let mut disc = disc;
    if is_wheel
        && let Some(d) = disc.take() {
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
