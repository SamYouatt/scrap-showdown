//! Procedural low-poly geometry. Everything is flat shaded with per-face colour
//! jitter + rust patches baked into vertex colours, which gives the chunky,
//! hand-painted junk look without any textures.
//!
//! Primitives produce convex-ish polygon soups (`Polys`), which are painted
//! into a `Geo` (coloured triangles). Geos are appended into each other with a
//! transform, so a whole part is one merged mesh (one draw call).
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::cell::Cell;
use std::f32::consts::{PI, TAU};

// ------------------------------------------------------------------ colour
/// Linear RGB colour (what vertex colours are stored as).
pub type Col = Vec3;

pub fn hexc(s: &str) -> Col {
    let c = Srgba::hex(s).unwrap_or(Srgba::WHITE);
    let l = LinearRgba::from(c);
    Vec3::new(l.red, l.green, l.blue)
}
pub fn col_of(c: Color) -> Col {
    let l = c.to_linear();
    Vec3::new(l.red, l.green, l.blue)
}
pub fn to_color(c: Col) -> Color {
    Color::linear_rgb(c.x, c.y, c.z)
}
pub fn darker(c: Col, f: f32) -> Col {
    c * f
}

pub struct Pal;
#[allow(non_upper_case_globals)]
impl Pal {
    pub fn rust() -> Col { hexc("#b8582a") }
    pub fn rust_dark() -> Col { hexc("#7a3a1e") }
    pub fn steel() -> Col { hexc("#9a9ca3") }
    pub fn steel_dark() -> Col { hexc("#5a5c64") }
    pub fn ink() -> Col { hexc("#2a2830") }
    pub fn tyre() -> Col { hexc("#2e2d33") }
    pub fn yellow() -> Col { hexc("#f2b632") }
    pub fn red() -> Col { hexc("#d8453a") }
    pub fn cream() -> Col { hexc("#efe2c4") }
    pub fn teal() -> Col { hexc("#3fa7a3") }
    pub fn wood() -> Col { hexc("#a8763e") }
}

// ------------------------------------------------------------------ seeded rand
// Model building reseeds this before generating a scene, so the same seed
// gives the same props. It's per-thread: reseed and build in one system.
thread_local! {
    static SEED: Cell<u64> = const { Cell::new(1337) };
}
/// Park–Miller, like the TS prototype (deterministic models per seed).
pub fn rand() -> f32 {
    SEED.with(|s| {
        let v = (s.get() * 16807) % 2147483647;
        s.set(v);
        (v - 1) as f32 / 2147483646.0
    })
}
pub fn reseed(s: u64) {
    SEED.with(|c| c.set((s % 2147483646).max(1)));
}

// ------------------------------------------------------------------ polygons
/// A planar convex polygon. Winding is fixed at paint time so the normal faces
/// away from `inside`.
#[derive(Clone)]
pub struct Poly {
    pub pts: Vec<Vec3>,
    pub inside: Vec3,
}
pub type Polys = Vec<Poly>;

fn poly(pts: Vec<Vec3>, inside: Vec3) -> Poly {
    Poly { pts, inside }
}

fn poly_normal(p: &[Vec3]) -> Vec3 {
    // Newell's method (robust for convex polys)
    let mut n = Vec3::ZERO;
    for i in 0..p.len() {
        let a = p[i];
        let b = p[(i + 1) % p.len()];
        n += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    n.normalize_or_zero()
}

/// Axis-aligned box centred at the origin.
pub fn box_polys(w: f32, h: f32, d: f32) -> Polys {
    chamfer_polys(w, h, d, 0.0)
}

/// Chamfered box (one-segment rounded box): the chunky bevelled look.
pub fn chamfer_polys(w: f32, h: f32, d: f32, r: f32) -> Polys {
    let hx = Vec3::new(w / 2.0, h / 2.0, d / 2.0);
    let a = hx - Vec3::splat(r);
    let mut out = vec![];
    let ax = |i: usize| -> Vec3 {
        let mut v = Vec3::ZERO;
        v[i] = 1.0;
        v
    };
    // faces
    for i in 0..3 {
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        for s in [-1.0f32, 1.0] {
            let p = |sj: f32, sk: f32| ax(i) * s * (a[i] + r) + ax(j) * sj * a[j] + ax(k) * sk * a[k];
            out.push(poly(vec![p(-1.0, -1.0), p(1.0, -1.0), p(1.0, 1.0), p(-1.0, 1.0)], Vec3::ZERO));
        }
    }
    if r <= 1e-5 {
        return out;
    }
    // edge chamfers
    for i in 0..3 {
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        for si in [-1.0f32, 1.0] {
            for sj in [-1.0f32, 1.0] {
                let pa = |t: f32| ax(i) * si * (a[i] + r) + ax(j) * sj * a[j] + ax(k) * t * a[k];
                let pb = |t: f32| ax(i) * si * a[i] + ax(j) * sj * (a[j] + r) + ax(k) * t * a[k];
                out.push(poly(vec![pa(-1.0), pa(1.0), pb(1.0), pb(-1.0)], Vec3::ZERO));
            }
        }
    }
    // corners
    for sx in [-1.0f32, 1.0] {
        for sy in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                let s = Vec3::new(sx, sy, sz);
                let c = a * s;
                out.push(poly(vec![c + Vec3::X * sx * r, c + Vec3::Y * sy * r, c + Vec3::Z * sz * r], Vec3::ZERO));
            }
        }
    }
    out
}

/// Cylinder/frustum along Y centred at origin (like THREE.CylinderGeometry).
pub fn cyl_polys(rt: f32, rb: f32, h: f32, seg: usize) -> Polys {
    let mut out = vec![];
    let ring = |r: f32, y: f32| -> Vec<Vec3> { (0..seg).map(|i| {
        let a = i as f32 / seg as f32 * TAU;
        Vec3::new(a.sin() * r, y, a.cos() * r)
    }).collect() };
    let top = ring(rt, h / 2.0);
    let bot = ring(rb, -h / 2.0);
    for i in 0..seg {
        let n = (i + 1) % seg;
        let mid = Vec3::new(0.0, (top[i].y + bot[i].y) / 2.0, 0.0);
        if rt <= 1e-5 {
            out.push(poly(vec![top[i], bot[i], bot[n]], mid));
        } else if rb <= 1e-5 {
            out.push(poly(vec![top[i], bot[i], top[n]], mid));
        } else {
            out.push(poly(vec![top[i], bot[i], bot[n], top[n]], mid));
        }
    }
    if rt > 1e-5 {
        out.push(poly(top.clone(), Vec3::ZERO));
    }
    if rb > 1e-5 {
        out.push(poly(bot.clone(), Vec3::ZERO));
    }
    out
}

pub fn ico_polys(r: f32) -> Polys {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let v: Vec<Vec3> = [
        (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
        (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
        (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
    ].iter().map(|&(x, y, z)| Vec3::new(x, y, z).normalize() * r).collect();
    const F: [[usize; 3]; 20] = [
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    F.iter().map(|f| poly(vec![v[f[0]], v[f[1]], v[f[2]]], Vec3::ZERO)).collect()
}

/// Low-poly torus in the XY plane (like THREE.TorusGeometry).
pub fn torus_polys(rr: f32, tube: f32, radial: usize, tubular: usize) -> Polys {
    let mut out = vec![];
    let p = |i: usize, j: usize| {
        let u = j as f32 / tubular as f32 * TAU;
        let v = i as f32 / radial as f32 * TAU;
        Vec3::new((rr + tube * v.cos()) * u.cos(), (rr + tube * v.cos()) * u.sin(), tube * v.sin())
    };
    for j in 0..tubular {
        for i in 0..radial {
            let u = (j as f32 + 0.5) / tubular as f32 * TAU;
            let c = Vec3::new(rr * u.cos(), rr * u.sin(), 0.0);
            out.push(poly(vec![p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1)], c));
        }
    }
    out
}

/// Extrude a convex (z, y) profile along X from -w/2..w/2.
pub fn extrude_zy(profile: &[(f32, f32)], w: f32) -> Polys {
    let c = profile.iter().fold(Vec3::ZERO, |a, &(z, y)| a + Vec3::new(0.0, y, z)) / profile.len() as f32;
    let mut out = vec![];
    let l: Vec<Vec3> = profile.iter().map(|&(z, y)| Vec3::new(-w / 2.0, y, z)).collect();
    let r: Vec<Vec3> = profile.iter().map(|&(z, y)| Vec3::new(w / 2.0, y, z)).collect();
    out.push(poly(l.clone(), c));
    out.push(poly(r.clone(), c));
    for i in 0..profile.len() {
        let n = (i + 1) % profile.len();
        out.push(poly(vec![l[i], l[n], r[n], r[i]], c));
    }
    out
}

/// Tube through a Catmull-Rom curve (cables).
pub fn tube_polys(pts: &[Vec3], r: f32, sides: usize) -> Polys {
    let samples = pts.len() * 4;
    let curve = |t: f32| -> Vec3 {
        let n = pts.len() - 1;
        let f = (t * n as f32).min(n as f32 - 1e-4);
        let i = f.floor() as usize;
        let u = f - i as f32;
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[(i + 1).min(n)];
        let p3 = pts[(i + 2).min(n)];
        let u2 = u * u;
        let u3 = u2 * u;
        0.5 * (2.0 * p1 + (-p0 + p2) * u + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3)
    };
    let cs: Vec<Vec3> = (0..=samples).map(|i| curve(i as f32 / samples as f32)).collect();
    let mut rings = vec![];
    let mut prev_n = Vec3::Y;
    for i in 0..cs.len() {
        let t = if i + 1 < cs.len() { cs[i + 1] - cs[i] } else { cs[i] - cs[i - 1] }.normalize_or(Vec3::Z);
        let mut n = (prev_n - t * prev_n.dot(t)).normalize_or_zero();
        if n == Vec3::ZERO {
            n = t.any_orthonormal_vector();
        }
        prev_n = n;
        let b = t.cross(n);
        rings.push((0..sides).map(|k| {
            let a = k as f32 / sides as f32 * TAU;
            cs[i] + (n * a.cos() + b * a.sin()) * r
        }).collect::<Vec<_>>());
    }
    let mut out = vec![];
    for i in 0..cs.len() - 1 {
        let c = (cs[i] + cs[i + 1]) / 2.0;
        for k in 0..sides {
            let kn = (k + 1) % sides;
            out.push(poly(vec![rings[i][k], rings[i + 1][k], rings[i + 1][kn], rings[i][kn]], c));
        }
    }
    out
}

// ------------------------------------------------------------------ painted geometry
/// Coloured, flat-shaded triangle soup.
#[derive(Clone, Default)]
pub struct Geo {
    pub pos: Vec<Vec3>,
    pub nrm: Vec<Vec3>,
    pub col: Vec<Col>,
}

impl Geo {
    pub fn new() -> Self {
        Self::default()
    }
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Col) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        self.pos.extend([a, b, c]);
        self.nrm.extend([n, n, n]);
        self.col.extend([col, col, col]);
    }
    /// Append another geo under transform `t`.
    pub fn add(&mut self, g: &Geo, t: Transform) -> &mut Self {
        let m = t.to_matrix();
        let nm = Mat3::from_mat4(m).inverse().transpose();
        for i in 0..g.pos.len() {
            self.pos.push(m.transform_point3(g.pos[i]));
            self.nrm.push((nm * g.nrm[i]).normalize_or_zero());
            self.col.push(g.col[i]);
        }
        self
    }
    pub fn add0(&mut self, g: &Geo) -> &mut Self {
        self.add(g, Transform::IDENTITY)
    }
    pub fn at(&mut self, g: &Geo, x: f32, y: f32, z: f32) -> &mut Self {
        self.add(g, Transform::from_xyz(x, y, z))
    }
    pub fn transformed(&self, t: Transform) -> Geo {
        let mut o = Geo::new();
        o.add(self, t);
        o
    }
    pub fn to_mesh(&self) -> Mesh {
        let n = self.pos.len() as u32;
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos.iter().map(|v| v.to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm.iter().map(|v| v.to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col.iter().map(|c| [c.x, c.y, c.z, 1.0]).collect::<Vec<_>>())
            .with_inserted_indices(Indices::U32((0..n).collect()))
    }
    pub fn tint(&mut self, f: Col) {
        for c in self.col.iter_mut() {
            *c *= f;
        }
    }
}

/// Paint polygons: per-face colour jitter + occasional rust.
pub fn paint(polys: &Polys, color: Col, rust: f32, jitter: f32) -> Geo {
    paint_ex(polys, color, rust, jitter, false)
}

fn paint_ex(polys: &Polys, color: Col, rust: f32, jitter: f32, edge_wear: bool) -> Geo {
    let mut g = Geo::new();
    let wear = hexc("#f3dcb4");
    for p in polys {
        let mut c = color;
        if rand() < rust {
            let r = if rand() < 0.5 { Pal::rust() } else { Pal::rust_dark() };
            c = c.lerp(r, 0.45 + rand() * 0.4);
        }
        c *= 1.0 + (rand() - 0.5) * 2.0 * jitter;
        let mut pts = p.pts.clone();
        let n = poly_normal(&pts);
        let centre = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        if n.dot(centre - p.inside) < 0.0 {
            pts.reverse();
        }
        if edge_wear {
            let nn = poly_normal(&pts);
            let axis = nn.x.abs().max(nn.y.abs()).max(nn.z.abs());
            // chipped, sun-bleached paint on the bevels that catch the light
            if axis < 0.97 && rand() >= 0.35 {
                let k = 0.18 + rand() * 0.18;
                let l = c.x * 0.3 + c.y * 0.59 + c.z * 0.11;
                c = (c * 1.25).lerp(wear, k * (l * 2.2).min(1.0));
            }
        }
        for i in 1..pts.len() - 1 {
            g.tri(pts[0], pts[i], pts[i + 1], c);
        }
    }
    g
}

// ------------------------------------------------------------------ primitive shortcuts
pub fn bx(w: f32, h: f32, d: f32, c: Col, rust: f32, jitter: f32) -> Geo {
    paint(&box_polys(w, h, d), c, rust, jitter)
}
/// Chamfered box with edge wear (TS: cbox).
pub fn cbox(w: f32, h: f32, d: f32, c: Col, bevel: f32, rust: f32) -> Geo {
    cbox_j(w, h, d, c, bevel, rust, 0.06)
}
pub fn cbox_j(w: f32, h: f32, d: f32, c: Col, bevel: f32, rust: f32, jitter: f32) -> Geo {
    let r = bevel.min(w / 2.2).min(h / 2.2).min(d / 2.2);
    paint_ex(&chamfer_polys(w, h, d, r), c, rust, jitter, r > 0.004)
}
/// Cylinder along Y.
pub fn cyl(rt: f32, rb: f32, h: f32, seg: usize, c: Col, rust: f32) -> Geo {
    paint(&cyl_polys(rt, rb, h, seg), c, rust, 0.07)
}
pub fn cyl_j(rt: f32, rb: f32, h: f32, seg: usize, c: Col, rust: f32, jitter: f32) -> Geo {
    paint(&cyl_polys(rt, rb, h, seg), c, rust, jitter)
}
pub fn ico(r: f32, c: Col) -> Geo {
    paint(&ico_polys(r), c, 0.0, 0.05)
}
/// Hex bolt head pointing along +Y.
pub fn bolt(r: f32, c: Col) -> Geo {
    paint(&cyl_polys(r, r, r * 0.8, 6), c, 0.0, 0.05)
}
/// Cylinder whose axis runs from a to b.
pub fn rod(a: Vec3, b: Vec3, r: f32, c: Col, seg: usize, rust: f32) -> Geo {
    let len = a.distance(b);
    paint(&cyl_polys(r, r, len, seg), c, rust, 0.05).transformed(Transform {
        translation: (a + b) / 2.0,
        rotation: Quat::from_rotation_arc(Vec3::Y, (b - a).normalize_or(Vec3::Y)),
        ..default()
    })
}
/// Saggy cable through points.
pub fn cable(pts: &[Vec3], r: f32, c: Col) -> Geo {
    paint(&tube_polys(pts, r, 4), c, 0.0, 0.04)
}
/// Row of bolts between two points with heads along `up`.
pub fn bolt_row(g: &mut Geo, a: Vec3, b: Vec3, n: usize, r: f32, up: Vec3, c: Col) {
    let q = Quat::from_rotation_arc(Vec3::Y, up);
    for i in 0..n {
        let t = if n == 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
        g.add(&bolt(r, c), Transform { translation: a.lerp(b, t), rotation: q, ..default() });
    }
}
/// Vent slats on a face lying in the XY plane facing +Z.
pub fn vents(g: &mut Geo, w: f32, h: f32, n: usize, c: Col, t: Transform) {
    for i in 0..n {
        let s = bx(w, h / (n as f32 * 2.0), 0.02, c, 0.0, 0.04);
        let y = -h / 2.0 + (i as f32 + 0.5) * (h / n as f32);
        g.add(&s, t * Transform::from_xyz(0.0, y, 0.0));
    }
}

/// Shorthands for transforms.
pub fn tr(x: f32, y: f32, z: f32) -> Transform {
    Transform::from_xyz(x, y, z)
}
pub fn trr(x: f32, y: f32, z: f32, rot: Quat) -> Transform {
    Transform { translation: Vec3::new(x, y, z), rotation: rot, ..default() }
}
pub fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}
pub fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}
pub fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}
pub const HALF_PI: f32 = PI / 2.0;

// ------------------------------------------------------------------ materials
pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Mats>();
}

/// Shared materials. `vc` is the vertex-coloured junk material used by
/// almost everything; glow materials bloom.
#[derive(Resource, Clone)]
pub struct Mats {
    pub vc: Handle<StandardMaterial>,
    /// battered, scorched paint (multiplies the vertex colours)
    pub scorch: Handle<StandardMaterial>,
}

impl FromWorld for Mats {
    fn from_world(world: &mut World) -> Self {
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        Mats {
            vc: mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.82, metallic: 0.08, reflectance: 0.35, ..default() }),
            scorch: mats.add(StandardMaterial { base_color: Color::srgb_u8(0x8a, 0x80, 0x7a), perceptual_roughness: 0.95, metallic: 0.05, ..default() }),
        }
    }
}

/// Unlit HDR material that blooms.
pub fn glow_mat(mats: &mut Assets<StandardMaterial>, c: Col, k: f32) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial {
        base_color: to_color(c * k),
        emissive: LinearRgba::rgb(c.x * k, c.y * k, c.z * k),
        unlit: true,
        ..default()
    })
}

// ------------------------------------------------------------------ props
pub fn tyre_prop() -> Geo {
    paint(&torus_polys(0.42, 0.2, 5, 9), Pal::tyre(), 0.0, 0.1)
}

pub fn barrel(c: Col) -> Geo {
    let mut g = cyl(0.32, 0.32, 0.9, 9, c, 0.35);
    g.at(&cyl(0.335, 0.335, 0.06, 9, darker(c, 0.7), 0.4), 0.0, 0.22, 0.0);
    g.at(&cyl(0.335, 0.335, 0.06, 9, darker(c, 0.7), 0.4), 0.0, -0.22, 0.0);
    g
}

pub fn car_shell(c: Col) -> Geo {
    let mut g = Geo::new();
    g.at(&bx(1.7, 0.55, 3.6, c, 0.4, 0.12), 0.0, 0.45, 0.0);
    g.at(&bx(1.5, 0.5, 1.8, c, 0.35, 0.12), 0.0, 0.95, -0.2);
    g.at(&bx(1.4, 0.4, 0.06, hexc("#7fb7c9"), 0.1, 0.07), 0.0, 0.95, 0.72);
    for sx in [-0.85, 0.85] {
        for sz in [-1.1, 1.1] {
            if rand() < 0.75 {
                g.add(&cyl(0.33, 0.33, 0.25, 8, Pal::tyre(), 0.0), trr(sx, 0.3, sz, rz(HALF_PI)));
            }
        }
    }
    g
}

pub fn crate_prop() -> Geo {
    let mut g = bx(0.8, 0.8, 0.8, Pal::wood(), 0.1, 0.12);
    for y in [-0.3, 0.3] {
        g.at(&bx(0.84, 0.1, 0.84, darker(Pal::wood(), 0.75), 0.1, 0.07), 0.0, y, 0.0);
    }
    g
}

pub fn washer() -> Geo {
    let mut g = bx(0.8, 0.9, 0.8, hexc("#e9e5dc"), 0.25, 0.08);
    g.add(&cyl(0.26, 0.26, 0.06, 10, Pal::steel_dark(), 0.0), trr(0.0, -0.05, 0.41, rx(HALF_PI)));
    g.add(&cyl(0.18, 0.18, 0.07, 10, hexc("#5c8aa0"), 0.0), trr(0.0, -0.05, 0.42, rx(HALF_PI)));
    g
}

pub fn fridge() -> Geo {
    let mut g = bx(0.8, 1.7, 0.75, hexc("#d9e1d6"), 0.3, 0.08);
    g.at(&bx(0.05, 0.5, 0.05, Pal::steel_dark(), 0.0, 0.07), 0.3, 0.3, 0.4);
    g
}

pub fn cloud() -> Geo {
    let mut g = Geo::new();
    let n = 4 + (rand() * 4.0) as usize;
    for i in 0..n {
        let r = 1.2 + rand() * 1.6;
        let s = paint(&ico_polys(r), Vec3::splat(1.0), 0.0, 0.03);
        g.add(&s, Transform { translation: Vec3::new((i as f32 - n as f32 / 2.0) * 1.5 + rand(), rand() * 0.8, rand() * 1.2), scale: Vec3::new(1.0, 0.7, 1.0), ..default() });
    }
    g
}
