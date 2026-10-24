//! Part catalogue + freeform (KSP-style) robot designs.
//!
//! A design is a tree of placed parts rooted at the Brain Box. Every part is
//! surface-attached: its local -Z face sits on a parent's surface and the part
//! extends along local +Z (the surface normal). Transforms are stored in
//! bot-local space (core centre = origin, +Z = forward, +X = driver's left).
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PartKind {
    Core,
    Block,
    Slab,
    Girder,
    Armour,
    Battery,
    Wheel,
    BigWheel,
    Leg,
    Spinner,
    Hammer,
    Flipper,
    Spike,
    Wedge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Structure,
    Drive,
    Weapon,
    Power,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Plate,
    Bolts,
}

/// Collider shapes in part-local space. Cylinders/cones are along local Z.
#[derive(Clone, Debug)]
pub enum Shape {
    Box { h: Vec3, c: Vec3 },
    Cyl { hh: f32, r: f32, c: Vec3 },
    Cone { hh: f32, r: f32, c: Vec3 },
    Hull { pts: Vec<Vec3> },
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub h: Vec3,
    pub c: Vec3,
}

pub struct PartDef {
    pub kind: PartKind,
    pub name: &'static str,
    pub colour: Color,
    pub category: Category,
    /// other parts may attach to it
    pub structural: bool,
    pub hp: f32,
    pub mass: f32,
    /// + supply, - demand
    pub power: f32,
    pub rarity: u8,
    pub repair: Res,
    pub desc: &'static str,
    pub bounds: Bounds,
}

const fn b3(hx: f32, hy: f32, hz: f32) -> Bounds {
    Bounds { h: Vec3::new(hx, hy, hz), c: Vec3::new(0.0, 0.0, hz) }
}
const fn bc(h: [f32; 3], c: [f32; 3]) -> Bounds {
    Bounds { h: Vec3::new(h[0], h[1], h[2]), c: Vec3::new(c[0], c[1], c[2]) }
}

/// Wedge profile in (z, y): back face against the surface, ramp down to a lip.
pub const WEDGE_PROFILE: [(f32, f32); 4] = [(0.0, -0.19), (0.0, 0.19), (0.62, -0.17), (0.62, -0.21)];

pub struct FlipCfg {
    pub hinge_y: f32,
    pub hinge_z: f32,
    pub len: f32,
    pub rest: f32,
    pub fire: f32,
}
pub const FLIP: FlipCfg = FlipCfg { hinge_y: -0.2, hinge_z: 0.15, len: 0.82, rest: 0.72, fire: -1.0 };
pub struct HammerCfg {
    pub pivot: Vec3,
    pub len: f32,
    pub rest: f32,
    pub strike: f32,
}
pub const HAMMER: HammerCfg = HammerCfg { pivot: Vec3::new(0.0, -0.1, 0.24), len: 0.7, rest: -0.35, strike: 2.35 };

pub const LEG_LEN: f32 = 0.66;

fn hex(s: &str) -> Color {
    Srgba::hex(s).map(Color::from).unwrap_or(Color::WHITE)
}

macro_rules! def {
    ($kind:ident, $name:expr, $col:expr, $cat:ident, $st:expr, $hp:expr, $mass:expr, $pow:expr, $rar:expr, $rep:ident, $desc:expr, $b:expr) => {
        PartDef {
            kind: PartKind::$kind,
            name: $name,
            colour: hex($col),
            category: Category::$cat,
            structural: $st,
            hp: $hp,
            mass: $mass,
            power: $pow,
            rarity: $rar,
            repair: Res::$rep,
            desc: $desc,
            bounds: $b,
        }
    };
}

pub fn def(kind: PartKind) -> &'static PartDef {
    use std::sync::OnceLock;
    static DEFS: OnceLock<Vec<PartDef>> = OnceLock::new();
    let defs = DEFS.get_or_init(|| {
        vec![
            def!(Core, "Brain Box", "#e2483d", Structure, true, 120.0, 6.0, 3.0, 3, Plate,
                "The heart of your bot. Lose this and it is game over. Supplies a little power.", bc([0.27, 0.2, 0.27], [0.0, 0.0, 0.0])),
            def!(Block, "Frame Block", "#8d8f95", Structure, true, 60.0, 2.2, 0.0, 0, Plate,
                "Chunky welded box section. The basic building block.", b3(0.25, 0.18, 0.25)),
            def!(Slab, "Chassis Rail", "#a2a6ad", Structure, true, 55.0, 2.0, 0.0, 0, Plate,
                "Long flat rail. Great for widening a chassis or pushing a weapon out front.", b3(0.25, 0.07, 0.45)),
            def!(Girder, "Girder Strut", "#d6a24a", Structure, true, 30.0, 0.7, 0.0, 0, Plate,
                "Light lattice boom. Reach for days – but leverage cuts both ways: wobbly turning, and it snaps.", b3(0.09, 0.09, 0.42)),
            def!(Armour, "Armour Plate", "#4cc3c1", Structure, true, 110.0, 3.6, 0.0, 0, Plate,
                "Hardened plate. Bolt it over anything you want to keep. Rattles loose when battered.", b3(0.3, 0.24, 0.04)),
            def!(Battery, "Car Battery", "#f6b73c", Power, true, 40.0, 3.0, 5.0, 1, Bolts,
                "+5 power. Underpowered bots walk slowly and swing weapons sluggishly.", b3(0.19, 0.14, 0.15)),
            def!(Wheel, "Pram Wheel", "#2b2b2e", Drive, false, 45.0, 1.4, -1.0, 2, Bolts,
                "Bolt to a side face so the axle points sideways. Mount it any other way and it rolls the wrong way!", bc([0.3, 0.3, 0.16], [0.0, 0.0, 0.2])),
            def!(BigWheel, "Tractor Wheel", "#3a3534", Drive, false, 80.0, 3.2, -1.5, 3, Bolts,
                "Huge knobbly tyre. Lots of grip and ground clearance, shrugs off hits.", bc([0.42, 0.42, 0.2], [0.0, 0.0, 0.26])),
            def!(Leg, "Stompy Leg", "#c8642e", Drive, false, 70.0, 2.6, -0.75, 0, Bolts,
                "Walks. Where you bolt it shapes the creature: on the sides it sprawls like a spider, underneath it stands tall. Wide stance = steady; two legs = wobbly. Useless upside down.", b3(0.15, 0.15, 0.15)),
            def!(Spinner, "Bar Spinner", "#e8e2d4", Weapon, false, 80.0, 5.0, -3.0, 3, Bolts,
                "Spinning saw disc. Spins up on its own and dumps its energy on contact. Mount it flat on top, or sideways for a vertical spinner.", bc([0.45, 0.45, 0.13], [0.0, 0.0, 0.13])),
            def!(Hammer, "Axe Hammer", "#e2483d", Weapon, false, 70.0, 4.0, -2.0, 2, Bolts,
                "Fire to swing. It chops towards the part's \"up\" arrow – mount it on top, on a side, or out on a strut for reach.", bc([0.2, 0.2, 0.1], [0.0, 0.0, 0.1])),
            def!(Flipper, "Pneumatic Flipper", "#f6b73c", Weapon, false, 70.0, 4.0, -2.5, 3, Bolts,
                "Fire to launch whatever is on the plate. Mount on top at the front edge so the lip reaches the floor.", bc([0.24, 0.24, 0.08], [0.0, 0.0, 0.08])),
            def!(Spike, "Ram Spike", "#c9ccd2", Weapon, false, 60.0, 1.8, -0.5, 1, Bolts,
                "Dumb, pointy, reliable. Damage scales with how fast you hit.", bc([0.15, 0.15, 0.08], [0.0, 0.0, 0.08])),
            def!(Wedge, "Scoop Wedge", "#7c8a46", Weapon, false, 90.0, 2.5, 0.0, 1, Plate,
                "Low slippery ramp. Mount on a front face; enemies that ride up it lose their grip.", bc([0.25, 0.19, 0.31], [0.0, 0.0, 0.31])),
        ]
    });
    &defs[kind as usize]
}

pub const KIND_ORDER: [PartKind; 13] = [
    PartKind::Block,
    PartKind::Slab,
    PartKind::Girder,
    PartKind::Armour,
    PartKind::Battery,
    PartKind::Wheel,
    PartKind::BigWheel,
    PartKind::Leg,
    PartKind::Spike,
    PartKind::Wedge,
    PartKind::Hammer,
    PartKind::Flipper,
    PartKind::Spinner,
];

impl PartKind {
    pub fn def(self) -> &'static PartDef {
        def(self)
    }
    pub fn is_wheel(self) -> bool {
        matches!(self, PartKind::Wheel | PartKind::BigWheel)
    }
}

// ---------------------------------------------------------------------------
#[derive(Clone, Debug)]
pub struct Placed {
    pub id: u32,
    pub kind: PartKind,
    pub parent: Option<u32>,
    pub pos: Vec3,
    pub rot: Quat,
    /// 0..1 persisted damage
    pub dmg: f32,
    /// id of the symmetric twin
    pub mirror: Option<u32>,
}

impl Placed {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rot, self.pos)
    }
}

#[derive(Clone, Debug)]
pub struct Design {
    pub name: String,
    pub paint: Color,
    pub parts: Vec<Placed>,
    pub next_id: u32,
}

impl Design {
    pub fn by_id(&self, id: u32) -> Option<&Placed> {
        self.parts.iter().find(|p| p.id == id)
    }
    pub fn by_id_mut(&mut self, id: u32) -> Option<&mut Placed> {
        self.parts.iter_mut().find(|p| p.id == id)
    }
    /// id and every descendant id.
    pub fn subtree(&self, id: u32) -> Vec<u32> {
        let mut out = vec![id];
        let mut grew = true;
        while grew {
            grew = false;
            for p in &self.parts {
                if let Some(par) = p.parent {
                    if out.contains(&par) && !out.contains(&p.id) {
                        out.push(p.id);
                        grew = true;
                    }
                }
            }
        }
        out
    }
}

/// Orientation for a part whose attach face sits on a surface with normal `n`
/// (bot-local). Local +Y defaults to "up" (or "forward" on horizontal faces);
/// roll spins about the normal, tilt leans the part over.
pub fn orient_on(n: Vec3, roll: f32, tilt: f32) -> Quat {
    let n = n.normalize();
    let q1 = Quat::from_rotation_arc(Vec3::Z, n);
    let r = if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::new(0.0, 0.0, if n.y > 0.0 { 1.0 } else { -1.0 }) };
    let target = (r - n * r.dot(n)).normalize();
    let y_now = q1 * Vec3::Y;
    let ang = y_now.cross(target).dot(n).atan2(y_now.dot(target));
    let q2 = Quat::from_axis_angle(n, ang);
    q2 * q1 * Quat::from_rotation_z(roll) * Quat::from_rotation_x(tilt)
}

/// Mirror a transform across the bot's centreline (x = 0).
pub fn mirror_xf(pos: Vec3, rot: Quat) -> (Vec3, Quat) {
    (Vec3::new(-pos.x, pos.y, pos.z), Quat::from_xyzw(rot.x, -rot.y, -rot.z, rot.w))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    PX,
    NX,
    PY,
    NY,
    PZ,
    NZ,
}

impl Face {
    pub const ALL: [Face; 6] = [Face::PX, Face::NX, Face::PY, Face::NY, Face::PZ, Face::NZ];
    pub fn axis(self) -> usize {
        match self {
            Face::PX | Face::NX => 0,
            Face::PY | Face::NY => 1,
            Face::PZ | Face::NZ => 2,
        }
    }
    pub fn sign(self) -> f32 {
        match self {
            Face::PX | Face::PY | Face::PZ => 1.0,
            _ => -1.0,
        }
    }
}

/// A point + outward normal on a face of a part's bounds box, in bot-local space.
pub fn face_point(p: &Placed, face: Face, u: f32, v: f32) -> (Vec3, Vec3) {
    let b = p.kind.def().bounds;
    let axis = face.axis();
    let sgn = face.sign();
    let mut local = b.c;
    let mut n = Vec3::ZERO;
    n[axis] = sgn;
    local[axis] = b.c[axis] + sgn * b.h[axis];
    let others: Vec<usize> = (0..3).filter(|a| *a != axis).collect();
    local[others[0]] += u;
    local[others[1]] += v;
    (p.rot * local + p.pos, p.rot * n)
}

// ---------------------------------------------------------------------------
/// Builder for hand-authored designs (opponents, starter bot).
pub struct Builder {
    pub d: Design,
}

#[derive(Default, Clone, Copy)]
pub struct AddOpts {
    pub u: f32,
    pub v: f32,
    pub roll: f32,
    pub tilt: f32,
    pub mirror: bool,
}

impl Builder {
    pub fn new(name: &str, paint: &str) -> Self {
        Builder {
            d: Design {
                name: name.into(),
                paint: hex(paint),
                parts: vec![Placed { id: 0, kind: PartKind::Core, parent: None, pos: Vec3::ZERO, rot: Quat::IDENTITY, dmg: 0.0, mirror: None }],
                next_id: 1,
            },
        }
    }
    pub fn add(&mut self, parent: u32, face: Face, kind: PartKind) -> u32 {
        self.add_o(parent, face, kind, AddOpts::default())
    }
    pub fn add_m(&mut self, parent: u32, face: Face, kind: PartKind) -> u32 {
        self.add_o(parent, face, kind, AddOpts { mirror: true, ..default() })
    }
    /// Attach `kind` to a face of `parent`. Returns the new id.
    pub fn add_o(&mut self, parent: u32, face: Face, kind: PartKind, o: AddOpts) -> u32 {
        let par = self.d.by_id(parent).unwrap().clone();
        let (point, normal) = face_point(&par, face, o.u, o.v);
        let q = orient_on(normal, o.roll, o.tilt);
        let id = self.d.next_id;
        self.d.next_id += 1;
        let mut p = Placed { id, kind, parent: Some(parent), pos: point, rot: q, dmg: 0.0, mirror: None };
        if o.mirror {
            let twin_parent = par.mirror.unwrap_or(par.id);
            let (mp, mr) = mirror_xf(p.pos, p.rot);
            let tid = self.d.next_id;
            self.d.next_id += 1;
            p.mirror = Some(tid);
            self.d.parts.push(p);
            self.d.parts.push(Placed { id: tid, kind, parent: Some(twin_parent), pos: mp, rot: mr, dmg: 0.0, mirror: Some(id) });
        } else {
            self.d.parts.push(p);
        }
        id
    }
}

// ---------------------------------------------------------------------------
// Stats

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub mass: f32,
    pub supply: f32,
    pub demand: f32,
    pub power_ratio: f32,
    pub drive: f32,
    pub speed: f32,
    pub armour: f32,
    pub bite: f32,
    pub balance: f32,
    pub com: Vec3,
    /// lowest ground-contact point (bot-local y)
    pub lowest: f32,
    pub warnings: Vec<String>,
}

pub fn wheel_radius(kind: PartKind) -> f32 {
    // to the tips of the tread lugs, so the hitbox is the tyre you see
    if kind == PartKind::BigWheel { 0.465 } else { 0.335 }
}

/// Bot-local wheel centre + axle for a wheel part.
pub fn wheel_frame(p: &Placed) -> (Vec3, Vec3) {
    let c = p.rot * Vec3::new(0.0, 0.0, p.kind.def().bounds.c.z) + p.pos;
    (c, p.rot * Vec3::Z)
}

pub fn lowest_point(p: &Placed) -> f32 {
    let b = p.kind.def().bounds;
    let m = p.matrix();
    let mut low = f32::INFINITY;
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let v = m.transform_point3(b.c + b.h * Vec3::new(sx, sy, sz));
                low = low.min(v.y);
            }
        }
    }
    low
}

/// How far below the core centre the bot touches the floor.
pub fn ground_level(d: &Design) -> f32 {
    let mut low = f32::INFINITY;
    for p in &d.parts {
        if p.kind.is_wheel() {
            let (c, axle) = wheel_frame(p);
            if axle.y.abs() < 0.6 {
                low = low.min(c.y - wheel_radius(p.kind) * (1.0 - axle.y * axle.y).sqrt());
            }
        } else if p.kind == PartKind::Leg {
            low = low.min(p.pos.y - LEG_LEN);
        }
    }
    if low == f32::INFINITY {
        for p in &d.parts {
            low = low.min(lowest_point(p));
        }
    }
    low
}

pub fn compute_stats(d: &Design) -> Stats {
    let (mut mass, mut supply, mut demand, mut armour_hp, mut bite, mut drive) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut com = Vec3::ZERO;
    let mut support: Vec<Vec3> = vec![];
    let mut bad_wheels = 0;
    let mut legs = 0;
    for p in &d.parts {
        let df = p.kind.def();
        mass += df.mass;
        com += p.pos * df.mass;
        if df.power > 0.0 { supply += df.power } else { demand -= df.power }
        if df.category == Category::Structure {
            armour_hp += df.hp * (1.0 - p.dmg);
        }
        if p.kind.is_wheel() {
            let (c, axle) = wheel_frame(p);
            if axle.y.abs() > 0.6 || axle.x.abs() < 0.5 {
                bad_wheels += 1;
            }
            if axle.y.abs() < 0.6 {
                drive += if p.kind == PartKind::BigWheel { 1.3 } else { 1.0 };
                support.push(c);
            }
        }
        if p.kind == PartKind::Leg {
            drive += 1.4;
            legs += 1;
            support.push(p.pos);
        }
        bite += match p.kind {
            PartKind::Spinner => 4.2,
            PartKind::Hammer => 2.6,
            PartKind::Flipper => 1.8,
            PartKind::Spike => 1.4,
            PartKind::Wedge => 0.6,
            _ => 0.0,
        };
    }
    com /= mass.max(0.001);
    let power_ratio = if demand <= 0.0 { 1.0 } else { (supply / demand).min(1.0) };
    let wheels = support.len() as f32 - legs as f32;
    let legs_f = legs as f32;
    let speed_raw = ((wheels * 150.0 + legs_f * 170.0) / mass.max(1.0)) * power_ratio * if legs_f > wheels { 0.55 } else { 1.0 };
    let speed = (speed_raw * 1.4).clamp(0.0, 10.0);
    let armour = (armour_hp / 75.0).clamp(0.0, 10.0);
    let bite_s = (bite * power_ratio * 1.3).clamp(0.0, 10.0);
    let mut warnings = vec![];
    let mut balance = 0.0;
    if !support.is_empty() {
        let sc = support.iter().copied().sum::<Vec3>() / support.len() as f32;
        balance = (10.0 - (com.x - sc.x).hypot(com.z - sc.z) * 14.0).max(0.0);
        // spread of the footprint vs how far the mass sticks out
        let mut inertia = 0.0;
        for p in &d.parts {
            inertia += p.kind.def().mass * ((p.pos.x - com.x).powi(2) + (p.pos.z - com.z).powi(2));
        }
        balance = (balance - (inertia / mass.max(1.0) - 0.45).max(0.0) * 5.0).max(0.0);
    }
    if drive == 0.0 {
        warnings.push("No working legs or wheels – it will just sit there.".into());
    } else if support.len() < 2 {
        warnings.push("One drive part. It will mostly spin in circles.".into());
    }
    if bad_wheels > 0 {
        warnings.push(format!("{bad_wheels} wheel(s) not on a side face – won't drive forwards."));
    }
    if power_ratio < 1.0 {
        warnings.push(format!("Underpowered ({supply}/{demand}). Add a battery."));
    }
    if bite == 0.0 {
        warnings.push("No weapons. Ramming only.".into());
    }
    Stats { mass, supply, demand, power_ratio, drive, speed, armour, bite: bite_s, balance, com, lowest: ground_level(d), warnings }
}
