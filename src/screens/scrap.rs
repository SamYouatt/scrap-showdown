//! The scrapheap: timed push-your-luck scavenging between fights. A radial
//! yard around the van; pile quality is rolled from distance (better further
//! out, but never certain). Camera-relative WASD, dig, bank at the van.
//!
//! The scavenger, the dog, the crane, piles and loose loot are entities with
//! their own components; [`Yard`] holds the clock and [`Haul`] what you're
//! carrying. When the whistle blows, [`Whistle`] switches to `Blown` and the
//! gameplay systems stop.
use super::Screen;
use crate::audio::{Sfx, Snd};
use crate::fx::Fx;
use crate::models::{self, bx, cbox, cbox_j, cyl, hexc, ico, rx, ry, tr, trr, Geo, Mats, Pal};
use crate::parts::{PartKind as K, Placed, Res as Stock};
use crate::preview::part_geo;
use crate::rng;
use crate::scene::{self, SunOpts};
use crate::state::{self, Run};
use crate::theme::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;
use std::f32::consts::{PI, TAU};
use std::time::Duration;

pub(super) fn plugin(app: &mut App) {
    app.add_sub_state::<Whistle>();
    app.add_systems(OnEnter(Screen::Scrap), spawn_yard);
    app.add_systems(OnExit(Screen::Scrap), |mut commands: Commands| {
        commands.remove_resource::<Yard>();
        commands.remove_resource::<Haul>();
        commands.remove_resource::<PileMats>();
        commands.remove_resource::<Fx>();
    });
    app.add_systems(
        Update,
        (
            (tick_clock, walk, dig, animate_scavenger, collect_drops, chase_dog, swing_crane, drop_car, bank_at_van, spin_glints, (show_dig_bar, show_pile_tag))
                .chain()
                .run_if(in_state(Whistle::Open)),
            refresh_hud.run_if(in_state(Whistle::Open).and_then(resource_changed::<Haul>.or_eager(on_timer(Duration::from_millis(250))))),
            follow_camera,
        )
            .chain()
            .run_if(in_state(Screen::Scrap)),
    );
    app.add_systems(OnEnter(Whistle::Blown), show_end_card);
}

/// Open while you scavenge; `Blown` once time's up or you leave early.
#[derive(SubStates, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[source(Screen = Screen::Scrap)]
enum Whistle {
    #[default]
    Open,
    Blown,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Loot {
    Plate,
    Bolts,
    Part(K),
}
impl Loot {
    fn name(self) -> &'static str {
        match self {
            Loot::Plate => "Scrap Plate",
            Loot::Bolts => "Bag of Bolts",
            Loot::Part(k) => k.def().name,
        }
    }
    fn rarity(self) -> usize {
        match self {
            Loot::Part(k) => (k.def().rarity as usize).min(3),
            _ => 0,
        }
    }
}

const T0: [(Loot, f32); 7] = [(Loot::Plate, 40.0), (Loot::Bolts, 36.0), (Loot::Part(K::Leg), 12.0), (Loot::Part(K::Girder), 7.0), (Loot::Part(K::Block), 5.0), (Loot::Part(K::Slab), 5.0), (Loot::Part(K::Wheel), 2.0)];
const T1: [(Loot, f32); 8] = [(Loot::Plate, 14.0), (Loot::Bolts, 14.0), (Loot::Part(K::Battery), 16.0), (Loot::Part(K::Spike), 12.0), (Loot::Part(K::Wedge), 12.0), (Loot::Part(K::Armour), 14.0), (Loot::Part(K::Leg), 12.0), (Loot::Part(K::Wheel), 4.0)];
const T2: [(Loot, f32); 8] = [(Loot::Part(K::Spinner), 20.0), (Loot::Part(K::Hammer), 20.0), (Loot::Part(K::Flipper), 18.0), (Loot::Part(K::Leg), 10.0), (Loot::Part(K::BigWheel), 8.0), (Loot::Part(K::Wheel), 6.0), (Loot::Part(K::Battery), 12.0), (Loot::Part(K::Armour), 12.0)];
const DIG_TIME: [f32; 3] = [1.1, 2.0, 4.2];
const ZONE_NAMES: [&str; 3] = ["The Yard", "The Heaps", "The Deep Pile"];
const ZONE_HEX: [&str; 3] = ["#9a9ca3", "#7b4ea8", "#f6b73c"];
const ZONE_DESC: [&str; 3] = ["Near the van. Mostly repair scrap, but you never know.", "Better odds out here. The crane is working!", "Best odds of rare parts. Beware of the dog..."];
const RARITY_HEX: [&str; 4] = ["#6a6c73", "#3e6e8a", "#7b4ea8", "#c78a14"];
const PILE_NAMES: [&str; 3] = ["Scrap pile", "Junk heap", "Treasure heap"];

const VAN: Vec3 = Vec3::ZERO;
const VAN_BODY: Vec3 = Vec3::new(-0.2, 0.0, 3.4);
const YARD_R: f32 = 27.0;
const SUN_DIR: Vec3 = Vec3::new(-14.0, 22.0, -10.0);
const RINGS: [f32; 2] = [11.0, 19.0];
const CRANE: Vec3 = Vec3::new(12.0, 0.0, 7.0);
const KENNEL: Vec3 = Vec3::new(-14.5, 0.0, 15.5);
const CABIN: Vec3 = Vec3::new(-6.5, 0.0, -2.5);
const SACK_MAX: usize = 6;

fn hex(h: &str) -> Color {
    Color::Srgba(Srgba::hex(h).unwrap())
}
fn wrap(a: f32) -> f32 {
    a.sin().atan2(a.cos())
}
fn ease(dt: f32, k: f32) -> f32 {
    1.0 - (-dt * k).exp()
}
/// pick with the seeded world-gen generator
fn wpick<T: Copy>(xs: &[T]) -> T {
    xs[((models::rand() * xs.len() as f32) as usize).min(xs.len() - 1)]
}

fn roll(zone: usize) -> Loot {
    let t: &[(Loot, f32)] = match zone {
        0 => &T0,
        1 => &T1,
        _ => &T2,
    };
    let tot: f32 = t.iter().map(|x| x.1).sum();
    let mut r = rng::rand() * tot;
    for (l, w) in t {
        r -= w;
        if r <= 0.0 {
            return *l;
        }
    }
    t[0].0
}

/// Pile quality from distance: mostly better further out, but never certain.
fn roll_tier(d: f32, r: f32) -> usize {
    let x = d / YARD_R;
    let w = [(1.15 - x * 1.4).max(0.05), 0.15 + x * 0.8, 0.04 + x * x * 1.2];
    let r = r * (w[0] + w[1] + w[2]);
    if r < w[0] { 0 } else if r < w[0] + w[1] { 1 } else { 2 }
}
fn ring_of(d: f32) -> usize {
    if d > RINGS[1] { 2 } else if d > RINGS[0] { 1 } else { 0 }
}

/// Flat white annulus in the XZ plane (tinted by its material).
fn ring_geo(r0: f32, r1: f32, seg: usize) -> Geo {
    let mut g = Geo::new();
    let p = |r: f32, i: usize| {
        let a = i as f32 / seg as f32 * TAU;
        Vec3::new(a.sin() * r, 0.0, a.cos() * r)
    };
    for i in 0..seg {
        for v in [p(r0, i), p(r1, i), p(r1, i + 1), p(r0, i), p(r1, i + 1), p(r0, i + 1)] {
            g.pos.push(v);
            g.nrm.push(Vec3::Y);
            g.col.push(Vec3::ONE);
        }
    }
    g
}

/// The yard's clock and layout.
#[derive(Resource)]
struct Yard {
    time_left: f32,
    zone: usize,
    /// the pile being dug, and for how long
    digging: Option<(Entity, f32)>,
    obstacles: Vec<(Vec3, f32)>,
    crane_piles: usize,
    early: bool,
    done: bool,
}

/// What you've found: in the sack, safe in the van, and what was still in
/// the sack when the whistle blew.
#[derive(Resource, Default)]
struct Haul {
    sack: Vec<Loot>,
    banked: Vec<Loot>,
    lost: Vec<Loot>,
}

#[derive(Resource)]
struct PileMats {
    ring: [Handle<StandardMaterial>; 3],
    glint: [Handle<StandardMaterial>; 3],
}

#[derive(Component)]
struct Scavenger {
    vel: Vec3,
    heading: f32,
    step_t: f32,
    stun: f32,
}

/// Parts of the scavenger rig that animate.
#[derive(Component, Clone, Copy, PartialEq)]
enum Rig {
    Body,
    Leg(usize),
    Arm(usize),
    Sack,
}

#[derive(Component)]
struct Pile {
    r: f32,
    zone: usize,
    dug: bool,
    glint: Entity,
    ring: Entity,
}

#[derive(Component)]
struct Glint {
    zone: usize,
}

/// Loot hopping out of a pile (or the sack), magnetised to the scavenger.
#[derive(Component)]
struct Drop {
    loot: Loot,
    vel: Vec3,
    t: f32,
    /// spilled loot has to be walked back to and picked up
    spilled: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum DogMood {
    Sleep,
    Chase,
    Return,
}

#[derive(Component)]
struct Dog {
    mood: DogMood,
    t: f32,
    bark: f32,
    home: Vec3,
}

#[derive(Component)]
struct DogLeg(usize);

/// The crane swings after you outside the yard and drops cars.
#[derive(Component)]
struct Crane {
    t: f32,
    target: Option<Vec3>,
    yaw: f32,
    magnet_pos: Vec3,
    carry: Option<Entity>,
}

#[derive(Component)]
struct Magnet;
#[derive(Component)]
struct Cable;
#[derive(Component)]
struct CraneWarning;
#[derive(Component)]
struct FallingCar {
    y: f32,
    vy: f32,
    rz: f32,
    yaw: f32,
}

#[derive(Component)]
struct YardCam {
    yaw: f32,
    target: Vec3,
}
#[derive(Component)]
struct HudCol;
#[derive(Component)]
struct DigBar;
#[derive(Component)]
struct DigFill;
#[derive(Component)]
struct TagNode;
#[derive(Component)]
struct TagText;

fn ment(c: &mut Commands, meshes: &mut Assets<Mesh>, m: &Handle<StandardMaterial>, g: &Geo, t: Transform) -> Entity {
    c.spawn((Mesh3d(meshes.add(g.to_mesh())), MeshMaterial3d(m.clone()), t, DespawnOnExit(Screen::Scrap))).id()
}

/// Spawns loose loot, with the fanfare for good finds.
#[derive(SystemParam)]
struct Loose<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    shared: Res<'w, Mats>,
    sfx: MessageWriter<'w, Sfx>,
    pops: MessageWriter<'w, PopWord>,
    toasts: MessageWriter<'w, Toast>,
}

impl Loose<'_, '_> {
    fn drop(&mut self, loot: Loot, at: Vec3, to_player: bool) {
        let g = match loot {
            Loot::Plate => bx(0.6, 0.06, 0.5, Pal::steel(), 0.4, 0.0),
            Loot::Bolts => {
                let mut g = ico(0.22, hexc("#b89a6a"));
                for _ in 0..4 {
                    g.add(&cyl(0.05, 0.05, 0.08, 6, Pal::steel(), 0.0), tr(rng::signed() * 0.15, 0.2, rng::signed() * 0.15));
                }
                g
            }
            Loot::Part(k) => {
                let pl = Placed { id: 0, kind: k, parent: None, pos: Vec3::ZERO, rot: Quat::IDENTITY, dmg: 0.0, mirror: None };
                part_geo(&pl, hexc("#c86a3a")).0.transformed(Transform::from_scale(Vec3::splat(0.9)))
            }
        };
        let e = ment(&mut self.commands, &mut self.meshes, &self.shared.vc, &g, Transform::from_translation(at));
        let vel = if to_player { Vec3::new(rng::signed(), 6.0, rng::signed()) } else { Vec3::new(rng::signed() * 3.0, 5.0, rng::signed() * 3.0) };
        self.commands.entity(e).insert((Name::new(loot.name()), Drop { loot, vel, t: if to_player { 0.0 } else { -1.0 }, spilled: !to_player }));
        if to_player {
            let r = loot.rarity();
            if r >= 2 {
                self.sfx.write(Snd::RareDing.play());
                let (word, color) = if r == 3 { ("JACKPOT!", YELLOW) } else { ("NICE!", hex("#c69cff")) };
                self.pops.write(PopWord { at: at.with_y(2.5), big: true, word: word.into(), color });
            } else {
                self.sfx.write(Snd::Ding.pitched(1.0 + r as f32 * 0.25));
            }
        }
    }

    /// Lose the most valuable things first. Ouch.
    fn spill(&mut self, haul: &mut Haul, from: Vec3, n: usize, why: &str) {
        haul.sack.sort_by_key(|l| std::cmp::Reverse(l.rarity()));
        for _ in 0..n.min(haul.sack.len()) {
            let l = haul.sack.remove(0);
            self.drop(l, from.with_y(1.2), false);
        }
        self.toasts.write(Toast(why.into(), RED));
    }
}

// ------------------------------------------------------------------ world
fn junk(pick: f32) -> Geo {
    let cars = ["#c86a3a", "#4a7aa8", "#7a8a4a", "#d8b03a"];
    if pick < 0.14 {
        return models::car_shell(hexc(wpick(&cars))).transformed(Transform::from_scale(Vec3::splat(0.45)));
    }
    if pick < 0.26 {
        return models::washer();
    }
    if pick < 0.32 {
        return models::fridge();
    }
    if pick < 0.44 {
        return models::tyre_prop();
    }
    if pick < 0.54 {
        return models::barrel(wpick(&[Pal::red(), Pal::teal(), Pal::yellow(), Pal::steel()]));
    }
    if pick < 0.6 {
        return models::crate_prop();
    }
    if pick < 0.7 {
        // engine block
        let mut e = cbox(0.7, 0.45, 0.5, Pal::steel_dark(), 0.04, 0.35);
        for k in 0..3 {
            e.at(&cyl(0.08, 0.08, 0.15, 6, Pal::steel(), 0.2), -0.2 + k as f32 * 0.2, 0.28, 0.0);
        }
        e.at(&cbox(0.3, 0.3, 0.06, Pal::rust(), 0.02, 0.5), 0.38, 0.0, 0.0);
        return e;
    }
    if pick < 0.8 {
        return models::rod(Vec3::new(-0.7, 0.0, 0.0), Vec3::new(0.7, 0.1, 0.0), 0.07, wpick(&[Pal::steel(), Pal::rust(), hexc("#3f7fd8")]), 7, 0.4);
    }
    if pick < 0.9 {
        return cbox(0.9, 0.05, 0.7, wpick(&[Pal::steel(), hexc("#d8453a"), hexc("#e8e0d0")]), 0.01, 0.5);
    }
    let mut g = Geo::new();
    for sy in [-0.08, 0.08] {
        g.at(&cbox(1.2, 0.04, 0.04, hexc("#d6a24a"), 0.01, 0.4), 0.0, sy, 0.0);
    }
    for k in 0..5 {
        g.at(&cbox(0.03, 0.16, 0.03, hexc("#d6a24a"), 0.005, 0.4), -0.5 + k as f32 * 0.25, 0.0, 0.0);
    }
    g
}

/// Static scenery: one shadow-casting mesh plus a flat decal mesh. Returns obstacles.
fn build_world(c: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, shared: &Mats, obs: &mut Vec<(Vec3, f32)>) {
    let mut stat = Geo::new();
    let mut flat = Geo::new();
    let r = models::rand;
    // concentric ground: concrete apron by the van, dirt further out
    flat.add(&cyl(80.0, 80.0, 1.0, 24, hexc("#b98c58"), 0.02), tr(0.0, -0.5, 0.0));
    for (rr, col) in [(RINGS[1], "#a97c4c"), (RINGS[0], "#c49866"), (5.5, "#a8a096")] {
        // every flat decal gets its own height so none share a plane
        let y = if rr < 6.0 { 0.012 } else if rr < RINGS[1] { 0.008 } else { 0.004 };
        flat.add(&cyl(rr, rr, 0.004, 40, hexc(col), 0.03), tr(0.0, y, 0.0));
    }
    // gravel tracks radiating from the van
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU + 0.3;
        flat.add(&bx(1.6, 0.004, YARD_R - 5.0, hexc("#9a8a74"), 0.0, 0.06), trr(a.sin() * (YARD_R / 2.0 + 2.5), 0.016 + i as f32 * 0.001, a.cos() * (YARD_R / 2.0 + 2.5), ry(a)));
    }
    // oil stains, puddles and dirt patches
    for i in 0..90 {
        let rad = 0.4 + r() * 1.8;
        let k = r();
        let col = if k < 0.2 { "#4a3e36" } else if k < 0.3 { "#6f8f9a" } else { "#b08254" };
        let (a, d) = (r() * TAU, 3.0 + r() * (YARD_R - 3.0));
        flat.add(&cyl(rad, rad, 0.004, 7, hexc(col), 0.0), tr(a.sin() * d, 0.024 + i as f32 * 0.0006, a.cos() * d));
    }
    // little bits of junk everywhere
    let bit_cols = [Pal::steel(), Pal::steel_dark(), Pal::rust(), Pal::tyre(), Pal::wood(), hexc("#d8453a"), hexc("#3f7fd8")];
    for _ in 0..260 {
        let (a, d) = (r() * TAU, 4.0 + r() * (YARD_R - 4.0));
        let k = r();
        let o = if k < 0.35 {
            cbox(0.3 + r() * 0.5, 0.04, 0.1 + r() * 0.3, wpick(&bit_cols), 0.01, 0.4)
        } else if k < 0.55 {
            models::rod(Vec3::ZERO, Vec3::new(0.6 + r(), 0.0, 0.0), 0.03, Pal::steel_dark(), 5, 0.4)
        } else if k < 0.7 {
            cyl(0.06, 0.06, 0.05, 6, Pal::steel(), 0.2)
        } else if k < 0.85 {
            cbox(0.18, 0.12, 0.14, wpick(&bit_cols), 0.02, 0.3)
        } else {
            models::tyre_prop().transformed(Transform { rotation: rx(PI / 2.0), scale: Vec3::splat(0.5), ..default() })
        };
        flat.add(&o, trr(a.sin() * d, 0.06, a.cos() * d, ry(r() * 6.0)));
    }
    // round perimeter fence
    let n_fence = 72;
    for i in 0..n_fence {
        let a0 = i as f32 / n_fence as f32 * TAU;
        let a1 = (i + 1) as f32 / n_fence as f32 * TAU;
        let p0 = Vec3::new(a0.sin(), 0.0, a0.cos()) * (YARD_R + 1.2);
        let p1 = Vec3::new(a1.sin(), 0.0, a1.cos()) * (YARD_R + 1.2);
        stat.add(&bx(0.15, 2.4, 0.15, Pal::steel_dark(), 0.3, 0.0), Transform::from_translation(p0.with_y(1.2)));
        let col = if r() < 0.5 { "#8a8f93" } else { "#7b6e60" };
        let rot = ry((p1.x - p0.x).atan2(p1.z - p0.z) + PI / 2.0);
        stat.add(&bx(p0.distance(p1), 2.0, 0.05, hexc(col), 0.45, 0.1), Transform { translation: p0.lerp(p1, 0.5).with_y(1.1), rotation: rot, ..default() });
    }
    // distant junk mountains (no shadows)
    for i in 0..26 {
        let a = i as f32 / 26.0 * TAU;
        let d = 40.0 + r() * 8.0;
        let col = if r() < 0.5 { "#8a6a52" } else { "#9a7a5a" };
        flat.add(&ico(6.0 + r() * 6.0, hexc(col)), Transform { translation: Vec3::new(a.cos() * d, -1.0, a.sin() * d), scale: Vec3::new(1.0, 0.55, 1.0), ..default() });
    }
    // ring walls of junk with gaps you walk through
    for (ri, rr) in RINGS.iter().enumerate() {
        let n = (rr * TAU / 3.1).round() as usize;
        for i in 0..n {
            if i % 4 == if ri > 0 { 1 } else { 2 } || r() < 0.2 {
                continue;
            }
            let a = i as f32 / n as f32 * TAU + ri as f32 * 0.2;
            let pos = Vec3::new(a.sin() * rr, 0.0, a.cos() * rr);
            if pos.distance(CRANE) < 4.0 || pos.distance(KENNEL) < 4.0 {
                continue;
            }
            let k = r();
            let j = if k < 0.45 {
                models::car_shell(hexc(wpick(&["#c86a3a", "#4a7aa8", "#7a8a4a", "#a84a4a", "#d8b03a"])))
            } else if k < 0.7 {
                let mut g = Geo::new();
                for h in 0..3 {
                    g.add(&models::tyre_prop(), trr(0.0, 0.2 + h as f32 * 0.4, 0.0, rx(PI / 2.0)));
                }
                g.at(&models::barrel(wpick(&[Pal::red(), Pal::teal(), Pal::yellow()])), 1.0, 0.45, 0.3);
                g
            } else {
                let mut g = Geo::new();
                g.at(&models::fridge(), -0.5, 0.85, 0.0);
                g.at(&models::washer(), 0.5, 0.45, 0.2);
                g
            };
            let mut rot = ry(a + PI / 2.0 + (r() - 0.5) * 0.6);
            if r() < 0.3 {
                rot *= models::rz(0.15);
            }
            stat.add(&j, Transform { translation: pos, rotation: rot, ..default() });
            obs.push((pos, 1.5));
        }
    }
    // landmark: towers of squashed cars
    for i in 0..5 {
        let a = i as f32 / 5.0 * TAU + 0.9;
        let pos = Vec3::new(a.sin() * (YARD_R - 2.0), 0.0, a.cos() * (YARD_R - 2.0));
        if pos.distance(KENNEL) < 5.0 {
            continue;
        }
        for h in 0..4 + (r() * 3.0) as usize {
            let col = hexc(wpick(&["#c86a3a", "#4a7aa8", "#7a8a4a", "#a84a4a", "#d8b03a", "#e8e0d0"]));
            stat.add(&cbox_j(3.2, 0.55, 1.6, col, 0.08, 0.45, 0.12), trr(pos.x, 0.28 + h as f32 * 0.56, pos.z, ry(a + (r() - 0.5) * 0.4)));
        }
        obs.push((pos, 1.8));
    }
    // zone boards at the ring edges, facing the van
    for (rr, col) in [(RINGS[0], "#7b4ea8"), (RINGS[1], "#c78a14")] {
        for a in [PI * 0.85, PI * 1.25, PI * 0.15] {
            let pos = Vec3::new(a.sin() * (rr - 0.6), 0.0, a.cos() * (rr - 0.6));
            let mut g = Geo::new();
            g.at(&bx(3.3, 0.7, 0.06, hexc(col), 0.2, 0.05), 0.0, 1.8, 0.0);
            g.at(&bx(2.6, 0.12, 0.07, hexc("#fff3dc"), 0.0, 0.0), 0.0, 1.8, 0.0);
            for dx in [-1.3, 1.3] {
                g.at(&bx(0.12, 1.8, 0.12, Pal::steel_dark(), 0.4, 0.0), dx, 0.9, 0.0);
            }
            stat.add(&g, Transform { translation: pos, rotation: ry(a), ..default() });
        }
    }
    // the van in the middle
    let mut van = Geo::new();
    van.at(&cbox_j(2.4, 1.6, 4.6, hexc("#e8e0d0"), 0.12, 0.15, 0.06), 0.0, 1.2, 0.0);
    van.at(&cbox_j(2.42, 0.5, 4.62, hexc("#3fa7a3"), 0.05, 0.1, 0.05), 0.0, 0.6, 0.0);
    van.at(&cbox(2.2, 0.7, 1.2, hexc("#e8e0d0"), 0.1, 0.1), 0.0, 0.95, -2.7);
    van.at(&bx(2.0, 0.55, 0.05, hexc("#7fb7c9"), 0.0, 0.0), 0.0, 1.75, -2.31);
    van.at(&bx(2.2, 0.08, 3.6, Pal::steel_dark(), 0.3, 0.0), 0.0, 2.05, 0.2);
    for sx in [-0.8, 0.0, 0.8] {
        van.at(&bx(0.06, 0.25, 3.6, Pal::steel_dark(), 0.3, 0.0), sx, 2.15, 0.2);
    }
    van.at(&cbox(0.7, 0.4, 0.6, Pal::wood(), 0.03, 0.2), -0.5, 2.3, 0.8);
    van.add(&models::tyre_prop(), Transform { translation: Vec3::new(0.5, 2.25, -0.4), rotation: rx(PI / 2.0), scale: Vec3::splat(0.8) });
    // big red logo on the side
    van.at(&cbox(0.06, 0.9, 1.2, hexc("#d8453a"), 0.02, 0.0), 1.22, 1.3, 0.4);
    for sx in [-1.15, 1.15] {
        for sz in [-1.8, 1.5] {
            van.add(&cyl(0.42, 0.42, 0.3, 9, Pal::tyre(), 0.0), trr(sx, 0.42, sz, models::rz(PI / 2.0)));
        }
    }
    stat.add(&van, trr(VAN_BODY.x, 0.0, VAN_BODY.z, ry(PI / 2.0)));
    // hazard-striped drop mat
    for i in 0..20 {
        let (a0, a1) = (i as f32 / 20.0 * TAU, (i + 1) as f32 / 20.0 * TAU);
        let col = if i % 2 == 0 { hexc("#3fa7a3") } else { hexc("#e8e0d0") };
        for v in [Vec3::ZERO, Vec3::new(a0.sin(), 0.0, a0.cos()) * 2.4, Vec3::new(a1.sin(), 0.0, a1.cos()) * 2.4] {
            flat.pos.push(VAN + v + Vec3::Y * 0.09);
            flat.nrm.push(Vec3::Y);
            flat.col.push(col);
        }
    }
    obs.push((VAN_BODY + Vec3::new(-1.6, 0.0, 0.0), 1.4));
    obs.push((VAN_BODY, 1.4));
    obs.push((VAN_BODY + Vec3::new(1.8, 0.0, 0.0), 1.4));
    // portacabin
    let mut cabin = Geo::new();
    cabin.at(&cbox_j(5.0, 2.6, 2.6, hexc("#d9b35a"), 0.06, 0.3, 0.06), 0.0, 1.5, 0.0);
    cabin.at(&bx(1.2, 0.8, 0.05, hexc("#7fb7c9"), 0.0, 0.0), -1.2, 1.7, -1.31);
    cabin.at(&bx(0.8, 1.8, 0.05, hexc("#7a4a2e"), 0.1, 0.0), 1.2, 1.1, -1.31);
    cabin.at(&cbox(5.3, 0.15, 2.9, Pal::steel_dark(), 0.03, 0.4), 0.0, 2.85, 0.0);
    stat.add(&cabin, trr(CABIN.x, 0.0, CABIN.z, ry(-0.5)));
    obs.push((CABIN + Vec3::new(-1.5, 0.0, 0.0), 1.6));
    obs.push((CABIN + Vec3::new(1.5, 0.0, 0.0), 1.6));
    // lamp posts + bunting
    let lamp_mat = models::glow_mat(mats, hexc("#fff2c0"), 2.4);
    let mut lamps = vec![];
    let mut glow = Geo::new();
    for i in 0..5 {
        let a = i as f32 / 5.0 * TAU + 1.23; // none straight between camera and van
        let pos = Vec3::new(a.sin() * 5.8, 0.0, a.cos() * 5.8);
        lamps.push(pos);
        obs.push((pos, 0.3));
        stat.add(&bx(0.15, 4.0, 0.15, Pal::steel_dark(), 0.2, 0.0), Transform::from_translation(pos.with_y(2.0)));
        stat.add(&bx(0.6, 0.15, 0.3, Pal::ink(), 0.1, 0.0), Transform::from_translation(pos.with_y(4.0)));
        glow.add(&bx(0.4, 0.06, 0.2, Vec3::ONE, 0.0, 0.0), Transform::from_translation(pos.with_y(3.92)));
    }
    let flag_cols = ["#d8453a", "#f2b632", "#3f7fd8", "#3fa7a3", "#e8e0d0"];
    for i in 0..lamps.len() {
        let (p0, p1) = (lamps[i].with_y(3.7), lamps[(i + 1) % lamps.len()].with_y(3.7));
        stat.add0(&models::cable(&[p0, p0.lerp(p1, 0.5).with_y(3.1), p1], 0.012, hexc("#2a2830")));
        for k in 1..9 {
            let u = k as f32 / 9.0;
            let q = p0.lerp(p1, u).with_y(3.7 - (u * PI).sin() * 0.6 - 0.12);
            let rot = ry((p1.x - p0.x).atan2(p1.z - p0.z)) * rx(PI);
            stat.add(&cyl(0.0, 0.12, 0.26, 3, hexc(flag_cols[k % 5]), 0.0), trr(q.x, q.y, q.z, rot));
        }
    }
    ment(c, meshes, &lamp_mat, &glow, Transform::IDENTITY);
    // kennel, door facing the van
    let mut kennel = Geo::new();
    kennel.at(&cbox_j(1.4, 1.0, 1.4, hexc("#a8763e"), 0.05, 0.1, 0.1), 0.0, 0.5, 0.0);
    kennel.add(&cyl(0.0, 1.15, 0.7, 4, hexc("#a84a4a"), 0.2), trr(0.0, 1.35, 0.0, ry(PI / 4.0)));
    kennel.at(&bx(0.6, 0.6, 0.05, Pal::ink(), 0.0, 0.0), 0.0, 0.35, -0.71);
    kennel.at(&cyl(0.25, 0.2, 0.12, 8, Pal::red(), 0.1), 0.9, 0.06, -0.6);
    stat.add(&kennel, trr(KENNEL.x, 0.0, KENNEL.z, ry(KENNEL.x.atan2(KENNEL.z))));
    obs.push((KENNEL, 1.0));
    // "beware of the dog" board
    let wp = (KENNEL * 0.9).lerp(Vec3::ZERO, 0.25);
    stat.add(&cbox(1.2, 0.5, 0.05, Pal::yellow(), 0.01, 0.2), trr(wp.x, 1.2, wp.z, ry(wp.x.atan2(wp.z))));
    stat.add(&bx(0.08, 1.2, 0.08, Pal::steel_dark(), 0.3, 0.0), Transform::from_translation(wp.with_y(0.6)));

    ment(c, meshes, &shared.vc, &stat, Transform::IDENTITY);
    let f = ment(c, meshes, &shared.vc, &flat, Transform::IDENTITY);
    c.entity(f).insert(bevy::light::NotShadowCaster);
}

fn add_pile(c: &mut Commands, meshes: &mut Assets<Mesh>, shared: &Mats, pm: &PileMats, p: Vec3, r: f32, zone: usize) {
    let mut g = Geo::new();
    let dirt = hexc(["#9a8060", "#8a6a4a", "#7e5236"][zone]);
    // lumpy mound from a few overlapping rocks
    for i in 0..3 {
        let a = models::rand() * TAU;
        let off = if i > 0 { Vec3::new(a.cos() * r * 0.5, -0.1, a.sin() * r * 0.5) } else { Vec3::new(0.0, -0.1, 0.0) };
        let lump = ico(r * if i > 0 { 0.6 } else { 1.0 }, dirt);
        g.add(&lump, Transform { translation: off, rotation: ry(models::rand() * 6.0), scale: Vec3::new(1.0, 0.42 + zone as f32 * 0.1, 1.0) });
    }
    for _ in 0..5 + zone * 3 {
        let o = junk(models::rand());
        let a = models::rand() * TAU;
        let d = models::rand().sqrt() * r * 0.7;
        let rot = Quat::from_euler(EulerRot::XYZ, (models::rand() - 0.5) * 1.2, models::rand() * 6.0, (models::rand() - 0.5) * 1.2);
        let y = r * (0.22 + zone as f32 * 0.05) * (1.0 - d / r) + 0.15;
        g.add(&o, Transform { translation: Vec3::new(a.cos() * d, y, a.sin() * d), rotation: rot, scale: Vec3::splat(0.8 + models::rand() * 0.4) });
    }
    if zone == 2 {
        // a tatty tarp half-covering the good stuff
        let rot = Quat::from_euler(EulerRot::XYZ, 0.25, models::rand() * 6.0, -0.3);
        g.add(&cbox_j(r * 1.1, 0.04, r * 0.9, hexc("#3f6fb0"), 0.02, 0.1, 0.12), trr(r * 0.2, r * 0.35, 0.0, rot));
    }
    // quality ring on the ground, readable from a distance
    let ring = ment(c, meshes, &pm.ring[zone], &ring_geo(r + 0.15, r + 0.35, 24), Transform::from_translation(p + Vec3::Y * 0.09));
    c.entity(ring).insert(bevy::light::NotShadowCaster);
    let glint = ment(c, meshes, &pm.glint[zone], &ico(0.16 + zone as f32 * 0.04, Vec3::ONE), Transform::from_xyz(p.x, 1.4 + zone as f32 * 0.35, p.z));
    c.entity(glint).insert((Glint { zone }, bevy::light::NotShadowCaster));
    let root = ment(c, meshes, &shared.vc, &g, Transform::from_translation(p));
    c.entity(root).insert((Name::new(PILE_NAMES[zone]), Pile { r, zone, dug: false, glint, ring }));
}

// ------------------------------------------------------------------ setup
#[allow(clippy::too_many_arguments)]
fn spawn_yard(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shared: Res<Mats>,
    fonts: Res<Fonts>,
    s: Res<Run>,
    mut toasts: MessageWriter<Toast>,
) {
    let scope = DespawnOnExit(Screen::Scrap);
    let c = &mut commands;
    let m = &mut *meshes;
    models::reseed(99);
    scene::sun_lights(c, SunOpts { size: 14.0, dir: SUN_DIR, sun: hex("#ffd49a"), intensity: 1.1, sky: hex("#94baf2"), ground: hex("#c08a52") }, scope.clone());
    scene::sky_and_clouds(c, m, &mut mats, hex("#5b9fe0"), hex("#ffe2b8"), 12, 70.0, scope.clone());
    let mut obstacles = vec![];
    build_world(c, m, &mut mats, &shared, &mut obstacles);

    let pm = PileMats {
        ring: [0, 1, 2].map(|z| mats.add(StandardMaterial { base_color: hex(ZONE_HEX[z]).with_alpha(if z > 0 { 0.7 } else { 0.35 }), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() })),
        glint: [0, 1, 2].map(|z| models::glow_mat(&mut mats, if z == 0 { hexc("#fff3dc") } else { hexc(ZONE_HEX[z]) }, 2.6)),
    };
    // piles: quality rolled from distance, so good stuff *can* be close
    let mut piles: Vec<(Vec3, f32)> = vec![];
    for (n, d0, d1) in [(9, 5.6, RINGS[0] - 1.3), (13, RINGS[0] + 1.3, RINGS[1] - 1.3), (13, RINGS[1] + 1.3, YARD_R - 2.0)] {
        let (mut made, mut tries) = (0, 0);
        while made < n && tries < 600 {
            tries += 1;
            let a = models::rand() * TAU;
            let d = d0 + models::rand() * (d1 - d0);
            let p = Vec3::new(a.sin() * d, 0.0, a.cos() * d);
            let tier = roll_tier(d, models::rand());
            let r = (1.1 + models::rand() * 0.7 + tier as f32 * 0.2).min((d1 - d0) / 2.0 + 0.3);
            if p.distance(CRANE) < 4.0 || p.distance(KENNEL) < 3.5 || p.distance(CABIN) < 4.5 {
                continue;
            }
            if piles.iter().any(|(q, qr)| q.distance(p) < qr + r + 1.6) || obstacles.iter().any(|(q, qr)| q.distance(p) < qr + r + 0.6) {
                continue;
            }
            add_pile(c, m, &shared, &pm, p, r, tier);
            piles.push((p, r));
            made += 1;
        }
    }

    // the scavenger: overalls, welding mask up, sack on the back
    let over = hexc("#3f6fb0");
    let mut bg = Geo::new();
    bg.at(&bx(0.5, 0.55, 0.32, over, 0.0, 0.08), 0.0, 0.85, 0.0);
    bg.at(&bx(0.52, 0.12, 0.34, hexc("#ee7a32"), 0.0, 0.05), 0.0, 1.0, 0.0);
    bg.at(&bx(0.34, 0.32, 0.32, hexc("#f1c7a1"), 0.0, 0.06), 0.0, 1.32, 0.0);
    bg.add(&bx(0.38, 0.3, 0.1, hexc("#2e2d33"), 0.0, 0.05), trr(0.0, 1.5, 0.12, rx(-0.9)));
    bg.at(&bx(0.4, 0.08, 0.36, hexc("#f2b632"), 0.0, 0.05), 0.0, 1.5, -0.02);
    for sx in [-0.07, 0.07] {
        bg.at(&bx(0.05, 0.07, 0.02, Pal::ink(), 0.0, 0.0), sx, 1.33, 0.165);
    }
    let start = Vec3::new(0.0, 0.0, -2.6);
    let player = c.spawn((Name::new("Scavenger"), Scavenger { vel: Vec3::ZERO, heading: 0.0, step_t: 0.0, stun: 0.0 }, Transform::from_translation(start), Visibility::default(), scope.clone())).id();
    let body = ment(c, m, &shared.vc, &bg, Transform::IDENTITY);
    c.entity(body).insert(Rig::Body);
    c.entity(player).add_child(body);
    let mut lg = Geo::new();
    lg.at(&bx(0.18, 0.5, 0.2, over, 0.0, 0.08), 0.0, -0.25, 0.0);
    lg.at(&bx(0.2, 0.12, 0.3, hexc("#4a3a2e"), 0.0, 0.05), 0.0, -0.52, 0.05);
    let mut ag = Geo::new();
    ag.at(&bx(0.14, 0.45, 0.16, over, 0.0, 0.08), 0.0, -0.2, 0.0);
    ag.at(&bx(0.15, 0.13, 0.17, hexc("#e8a63a"), 0.0, 0.05), 0.0, -0.45, 0.0);
    for (i, sx) in [-0.13, 0.13].into_iter().enumerate() {
        let e = ment(c, m, &shared.vc, &lg, tr(sx, 0.6, 0.0));
        c.entity(e).insert((Rig::Leg(i), ChildOf(body)));
    }
    for (i, sx) in [-0.33, 0.33].into_iter().enumerate() {
        let e = ment(c, m, &shared.vc, &ag, tr(sx, 1.08, 0.0));
        c.entity(e).insert((Rig::Arm(i), ChildOf(body)));
    }
    let sack = ment(c, m, &shared.vc, &ico(0.3, hexc("#b89a6a")), tr(0.0, 1.0, -0.32));
    c.entity(sack).insert((Rig::Sack, ChildOf(body)));

    // the yard dog
    let fur = hexc("#7a5a3a");
    let mut dg = Geo::new();
    dg.at(&bx(0.45, 0.4, 0.9, fur, 0.05, 0.1), 0.0, 0.55, 0.0);
    let hd = Vec3::new(0.0, 0.85, 0.5);
    dg.at(&bx(0.36, 0.34, 0.36, fur, 0.05, 0.1), hd.x, hd.y, hd.z);
    dg.at(&bx(0.22, 0.18, 0.22, hexc("#c8a27a"), 0.0, 0.05), 0.0, hd.y - 0.06, hd.z + 0.24);
    dg.at(&bx(0.1, 0.08, 0.06, Pal::ink(), 0.0, 0.0), 0.0, hd.y, hd.z + 0.36);
    for sx in [-0.12, 0.12] {
        dg.at(&bx(0.08, 0.2, 0.06, hexc("#5a3a22"), 0.0, 0.05), sx, hd.y + 0.22, hd.z - 0.05);
    }
    dg.at(&bx(0.4, 0.08, 0.4, Pal::red(), 0.0, 0.05), 0.0, hd.y - 0.2, hd.z - 0.05);
    dg.add(&bx(0.08, 0.08, 0.35, fur, 0.0, 0.0), trr(0.0, 0.8, -0.55, rx(0.7)));
    let dog_home = KENNEL * 0.9;
    let dog = ment(c, m, &shared.vc, &dg, Transform::from_translation(dog_home).with_rotation(ry(PI)));
    c.entity(dog).insert((Name::new("Dog"), Dog { mood: DogMood::Sleep, t: 0.0, bark: 0.0, home: dog_home }));
    let eye_mat = models::glow_mat(&mut mats, hexc("#ffe14a"), 2.4);
    let mut eg = Geo::new();
    for sx in [-0.12, 0.12] {
        eg.at(&bx(0.06, 0.06, 0.02, Vec3::ONE, 0.0, 0.0), sx * 0.8, hd.y + 0.06, hd.z + 0.185);
    }
    let eyes = ment(c, m, &eye_mat, &eg, Transform::IDENTITY);
    c.entity(eyes).insert(ChildOf(dog));
    let mut dl = Geo::new();
    dl.at(&bx(0.12, 0.4, 0.12, fur, 0.0, 0.0), 0.0, -0.2, 0.0);
    for (i, (sx, sz)) in [(-0.15, 0.3), (0.15, 0.3), (-0.15, -0.3), (0.15, -0.3)].into_iter().enumerate() {
        let e = ment(c, m, &shared.vc, &dl, tr(sx, 0.4, sz));
        c.entity(e).insert((DogLeg(i), ChildOf(dog)));
    }

    // the crane: tracks, a turning cab and a lattice boom with a magnet
    let yel = hexc("#f2b632");
    ment(c, m, &shared.vc, &bx(3.0, 0.8, 3.4, Pal::ink(), 0.1, 0.0), Transform::from_translation(CRANE + Vec3::Y * 0.4));
    let mut top = Geo::new();
    top.at(&bx(2.4, 1.6, 2.6, yel, 0.25, 0.08), 0.0, 1.6, -0.3);
    top.at(&bx(1.2, 1.0, 1.0, hexc("#7fb7c9"), 0.05, 0.0), 0.5, 2.6, 0.6);
    let mut boom = Geo::new();
    for i in 0..9 {
        let z = i as f32 * 1.1 + 0.6;
        for sx in [-0.3, 0.3] {
            for sy in [-0.3, 0.3] {
                boom.at(&bx(0.08, 0.08, 1.2, yel, 0.3, 0.0), sx, sy, z);
            }
            boom.add(&bx(0.06, 0.8, 0.06, yel, 0.3, 0.0), trr(sx, 0.0, z, rx(0.7)));
        }
    }
    top.add(&boom, trr(0.0, 2.4, 0.6, rx(-0.45)));
    let magnet_pos = CRANE + Vec3::new(0.0, 6.5, 10.0);
    let crane = ment(c, m, &shared.vc, &top, Transform::from_translation(CRANE));
    c.entity(crane).insert((Name::new("Crane"), Crane { t: 8.0, target: None, yaw: 0.0, magnet_pos, carry: None }));
    obstacles.push((CRANE, 2.2));
    let mut mg = cyl(0.9, 0.9, 0.35, 12, Pal::steel_dark(), 0.3);
    mg.at(&cyl(0.2, 0.2, 0.3, 6, Pal::red(), 0.1), 0.0, 0.3, 0.0);
    let magnet = ment(c, m, &shared.vc, &mg, Transform::from_translation(magnet_pos));
    c.entity(magnet).insert(Magnet);
    let cable = ment(c, m, &shared.vc, &cyl(0.03, 0.03, 1.0, 4, hexc("#2a2830"), 0.0), Transform::IDENTITY);
    c.entity(cable).insert(Cable);
    let warn_mat = mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.23, 0.1, 0.45), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    let warn = ment(c, m, &warn_mat, &ring_geo(1.2, 1.6, 24), Transform::IDENTITY);
    c.entity(warn).insert((CraneWarning, Visibility::Hidden, bevy::light::NotShadowCaster));

    let fx = Fx::new(c, m, &mut mats, |_, _| 0.0, scope.clone());
    c.insert_resource(fx);
    c.spawn((
        scene::camera_bundle(42.0, Transform::from_xyz(0.0, 9.5, 4.6).looking_at(start.with_y(0.4), Vec3::Y)),
        DistanceFog { color: hex("#f2d6b0"), falloff: FogFalloff::Linear { start: 30.0, end: 80.0 }, ..default() },
        // looks outward from the van: W heads out, S heads home
        YardCam { yaw: start.x.atan2(start.z), target: start },
        scope.clone(),
    ));

    // UI: HUD column, hint, dig bar, pile tag
    c.spawn((Name::new("Yard HUD"), Node { position_type: PositionType::Absolute, top: Val::Px(14.0), left: Val::Px(14.0), width: Val::Px(280.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), ..default() }, Pickable::IGNORE, HudCol, scope.clone()));
    c.spawn((
        widget::ui_root("Hint", Node { bottom: Val::Px(14.0), justify_content: JustifyContent::Center, ..default() }),
        scope.clone(),
        children![(
            widget::panel(Node { padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)), ..default() }),
            Pickable::IGNORE,
            children![(widget::text(&fonts, "WASD move · hold E / SPACE by a pile to dig · drop loot at the van to keep it · ENTER at the van to leave early", 16.0, CREAM), Pickable::IGNORE)],
        )],
    ));
    c.spawn((
        Name::new("Dig bar"),
        Node { position_type: PositionType::Absolute, width: Val::Px(80.0), height: Val::Px(12.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(6.0)), display: Display::None, ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        BorderColor::all(CREAM),
        Pickable::IGNORE,
        DigBar,
        scope.clone(),
        children![(Node { width: Val::Percent(0.0), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, BackgroundColor(YELLOW), DigFill, Pickable::IGNORE)],
    ));
    c.spawn((
        Name::new("Pile tag"),
        Node { position_type: PositionType::Absolute, width: Val::Px(240.0), justify_content: JustifyContent::Center, display: Display::None, ..default() },
        Pickable::IGNORE,
        TagNode,
        scope,
        children![(
            Node { padding: UiRect::axes(Val::Px(9.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
            BackgroundColor(Color::srgba(0.1, 0.09, 0.13, 0.85)),
            Pickable::IGNORE,
            children![(widget::text(&fonts, "", 14.0, CREAM), TextLayout::default().with_no_wrap(), TagText, Pickable::IGNORE)],
        )],
    ));

    let seconds = if s.scrap_time > 0.0 { s.scrap_time } else { 60.0 };
    toasts.write(Toast(format!("You have {}s. Get back to the van before the whistle!", seconds.round()), YELLOW));
    c.insert_resource(pm);
    c.insert_resource(Haul::default());
    c.insert_resource(Yard { time_left: seconds, zone: 0, digging: None, obstacles, crane_piles: 0, early: false, done: false });
}

// ------------------------------------------------------------------ gameplay
fn tick_clock(time: Res<Time>, mut yard: ResMut<Yard>, mut haul: ResMut<Haul>, player: Single<&Transform, With<Scavenger>>, mut whistle: ResMut<NextState<Whistle>>, mut sfx: MessageWriter<Sfx>) {
    yard.time_left -= time.delta_secs().min(0.05);
    let z = ring_of(player.translation.length());
    if z != yard.zone {
        yard.zone = z;
    }
    if yard.time_left <= 0.0 {
        blow_whistle(&mut haul, &mut whistle, &mut sfx);
    }
}

/// Time's up (or you left): whatever is still in the sack is lost.
fn blow_whistle(haul: &mut Haul, whistle: &mut NextState<Whistle>, sfx: &mut MessageWriter<Sfx>) {
    haul.lost = haul.sack.clone();
    whistle.set(Whistle::Blown);
    sfx.write(Snd::Bell.play());
}

/// Camera-relative movement, pushed out of obstacles and piles.
fn walk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    yard: Res<Yard>,
    haul: Res<Haul>,
    cam: Single<&YardCam>,
    piles: Query<(&Transform, &Pile), Without<Scavenger>>,
    player: Single<(&mut Transform, &mut Scavenger)>,
    mut fx: ResMut<Fx>,
) {
    let dt = time.delta_secs().min(0.05);
    let (mut tf, mut sc) = player.into_inner();
    let k = |cs: &[KeyCode]| cs.iter().any(|x| keys.pressed(*x));
    let ix = k(&[KeyCode::KeyD, KeyCode::ArrowRight]) as i32 - k(&[KeyCode::KeyA, KeyCode::ArrowLeft]) as i32;
    let iz = k(&[KeyCode::KeyW, KeyCode::ArrowUp]) as i32 - k(&[KeyCode::KeyS, KeyCode::ArrowDown]) as i32;
    let f = Vec3::new(cam.yaw.sin(), 0.0, cam.yaw.cos());
    let right = Vec3::new(-f.z, 0.0, f.x);
    let mut want = (f * iz as f32 + right * ix as f32).normalize_or_zero();
    let speed = 6.2 * (1.0 - haul.sack.len() as f32 * 0.06);
    if sc.stun > 0.0 {
        sc.stun -= dt;
        want = Vec3::ZERO;
    }
    sc.vel = sc.vel.lerp(want * speed, ease(dt, 12.0));
    let mut pos = tf.translation + sc.vel * dt;
    let solids = yard.obstacles.iter().copied().chain(piles.iter().map(|(t, p)| (t.translation, p.r * 0.85)));
    for (o, r) in solids {
        let d = (pos - o).with_y(0.0);
        let l = d.length();
        let min = r + 0.35;
        if l < min && l > 0.001 {
            pos += d / l * (min - l);
        }
    }
    if pos.length() > YARD_R - 0.5 {
        pos = pos.normalize() * (YARD_R - 0.5);
    }
    tf.translation = pos;
    let sp = sc.vel.length();
    if sp > 0.3 {
        let h = sc.vel.x.atan2(sc.vel.z);
        sc.heading += wrap(h - sc.heading) * ease(dt, 14.0);
        let prev = sc.step_t;
        sc.step_t += dt * sp * 2.2;
        if (sc.step_t / PI).floor() != (prev / PI).floor() {
            fx.dust_puff(pos.with_y(0.05), 1, None, 0.1, 0.5);
        }
    }
}

/// Hold E / SPACE by a pile to dig it out.
#[allow(clippy::too_many_arguments)]
fn dig(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut yard: ResMut<Yard>,
    haul: Res<Haul>,
    player: Single<(&Transform, &mut Scavenger)>,
    mut piles: Query<(Entity, &mut Transform, &mut Pile), Without<Scavenger>>,
    mut vis: Query<&mut Visibility>,
    mut fx: ResMut<Fx>,
    mut loose: Loose,
) {
    let dt = time.delta_secs().min(0.05);
    let (ptf, mut sc) = player.into_inner();
    let pos = ptf.translation;
    let nearest = piles
        .iter()
        .filter(|(_, t, p)| !p.dug && t.translation.distance(pos) - p.r < 1.0)
        .min_by(|a, b| (a.1.translation.distance(pos) - a.2.r).total_cmp(&(b.1.translation.distance(pos) - b.2.r)))
        .map(|(e, ..)| e);
    let holding = (keys.pressed(KeyCode::KeyE) || keys.pressed(KeyCode::Space)) && sc.stun <= 0.0;
    let Some(pe) = nearest.filter(|_| holding && sc.vel.length() < 1.5) else {
        yard.digging = None;
        return;
    };
    if haul.sack.len() >= SACK_MAX {
        if keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::Space) {
            loose.toasts.write(Toast("Sack is full! Drop it at the van.".into(), RED));
        }
        yard.digging = None;
        return;
    }
    let dug_for = match yard.digging {
        Some((e, t)) if e == pe => t + dt,
        _ => dt,
    };
    yard.digging = Some((pe, dug_for));
    let (_, mut pile_tf, mut pile) = piles.get_mut(pe).unwrap();
    let pp = pile_tf.translation;
    sc.heading = (pp.x - pos.x).atan2(pp.z - pos.z);
    if rng::chance(dt * 12.0) {
        let p = pos.lerp(pp, 0.5).with_y(0.4);
        fx.dust_puff(p, 1, Some(hexc("#a08060")), 0.12, 1.5);
        if rng::chance(0.3) {
            fx.spark_burst(p, 2, None, 3.0, 1.0);
        }
        fx.bolt_burst(p, 1, None);
    }
    if rng::chance(dt * 5.0) {
        loose.sfx.write(Snd::Scrape.play());
    }
    if dug_for < DIG_TIME[pile.zone] {
        return;
    }
    // dug out: sink the pile, hide its marker, fling the loot
    pile.dug = true;
    yard.digging = None;
    for e in [pile.glint, pile.ring] {
        if let Ok(mut v) = vis.get_mut(e) {
            *v = Visibility::Hidden;
        }
    }
    pile_tf.scale.y = 0.65;
    let zone = pile.zone;
    let n = if zone == 0 && rng::chance(0.35) { 2 } else { 1 };
    let at = pp.lerp(pos, 0.6).with_y(1.0);
    for _ in 0..n {
        loose.drop(roll(zone), at, true);
    }
    fx.dust_puff(pp.with_y(0.5), 14, Some(hexc("#a08060")), 0.3, 3.0);
    loose.sfx.write(Snd::Thud.at(0.4));
}

/// Walk cycle, digging arms, stun wobble, a sack that fills out.
fn animate_scavenger(time: Res<Time>, yard: Res<Yard>, haul: Res<Haul>, player: Single<(&mut Transform, &Scavenger), Without<Rig>>, mut rig: Query<(&Rig, &mut Transform)>) {
    let t = time.elapsed_secs();
    let (mut tf, sc) = player.into_inner();
    tf.rotation = ry(sc.heading);
    let sp = sc.vel.length();
    let swing = sc.step_t.sin() * (sp / 3.0).min(1.0) * 0.7;
    let arm = if yard.digging.is_some() { [-1.2 + (t * 22.0).sin() * 0.6, -1.2 - (t * 22.0).sin() * 0.6] } else { [-swing, swing] };
    for (part, mut tf) in &mut rig {
        match *part {
            Rig::Body => {
                tf.translation.y = sc.step_t.sin().abs() * 0.06 * (sp / 3.0).min(1.0);
                tf.rotation = models::rz(if sc.stun > 0.0 { (t * 20.0).sin() * 0.2 } else { 0.0 });
            }
            Rig::Leg(i) => tf.rotation = rx(if i == 0 { swing } else { -swing }),
            Rig::Arm(i) => tf.rotation = rx(arm[i]),
            Rig::Sack => {
                let n = haul.sack.len() as f32;
                tf.scale = Vec3::splat(0.6 + n * 0.18);
                tf.translation = Vec3::new(0.0, 1.0 + n * 0.03, -0.3 - n * 0.04);
            }
        }
    }
}

/// Loot hops out, then flies into the sack (spilled loot only when you're close).
#[allow(clippy::too_many_arguments)]
fn collect_drops(
    mut commands: Commands,
    time: Res<Time>,
    mut haul: ResMut<Haul>,
    player: Single<(&Transform, &Scavenger), Without<Drop>>,
    mut drops: Query<(Entity, &mut Drop, &mut Transform)>,
    mut toasts: MessageWriter<Toast>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();
    let (ptf, sc) = *player;
    let ppos = ptf.translation.with_y(1.0);
    for (i, (e, mut d, mut tf)) in drops.iter_mut().enumerate() {
        let full = haul.sack.len() >= SACK_MAX;
        d.t += dt;
        d.vel.y -= 18.0 * dt;
        let mut pos = tf.translation + d.vel * dt;
        if pos.y < 0.25 {
            pos.y = 0.25;
            d.vel.y = d.vel.y.abs() * 0.3;
            d.vel.x *= 0.7;
            d.vel.z *= 0.7;
        }
        tf.translation = pos;
        tf.rotation = ry(t * 3.0 + i as f32);
        let to_p = ppos - pos;
        if d.t <= 0.45 || sc.stun > 0.0 {
            continue;
        }
        if to_p.length() < 2.2 || (!d.spilled && d.t < 1.2) {
            d.vel = d.vel.lerp(to_p.normalize_or_zero() * 14.0, ease(dt, 10.0));
        }
        if to_p.length() >= 0.6 {
            continue;
        }
        if full {
            if d.t <= 1.2 {
                d.t = -2.0;
                d.vel = Vec3::new(rng::signed() * 2.0, 4.0, rng::signed() * 2.0);
                toasts.write(Toast("Sack full!".into(), RED));
            }
            continue;
        }
        commands.entity(e).despawn();
        haul.sack.push(d.loot);
        toasts.write(Toast(d.loot.name().into(), hex(RARITY_HEX[d.loot.rarity()])));
        sfx.write(Snd::Pop.play());
    }
}

/// Digging is loud: the dog wakes, chases, and bites if it catches you.
#[allow(clippy::too_many_arguments)]
fn chase_dog(
    time: Res<Time>,
    mut yard: ResMut<Yard>,
    mut haul: ResMut<Haul>,
    player: Single<(&mut Transform, &mut Scavenger), Without<Dog>>,
    dog: Single<(&mut Transform, &mut Dog)>,
    mut legs: Query<(&DogLeg, &mut Transform), (Without<Dog>, Without<Scavenger>)>,
    mut fx: ResMut<Fx>,
    mut loose: Loose,
    mut flash: MessageWriter<Flash>,
) {
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();
    let (mut ptf, mut sc) = player.into_inner();
    let (mut dtf, mut dog) = dog.into_inner();
    let mut dog_pos = dtf.translation.with_y(0.0);
    let to_p = (ptf.translation - dog_pos).with_y(0.0);
    let dist = to_p.length();
    dog.t += dt;
    let mut bob = 0.0;
    match dog.mood {
        DogMood::Sleep => {
            bob = (t * 2.0).sin() * 0.02;
            if dist < 7.0 || (yard.digging.is_some() && dist < 12.0) {
                dog.mood = DogMood::Chase;
                dog.t = 0.0;
                loose.sfx.write(Snd::Bark.play());
                loose.pops.write(PopWord { at: dog_pos.with_y(1.6), big: true, word: "WOOF!".into(), color: hex("#ff8a5c") });
            }
        }
        DogMood::Chase => {
            // an empty-handed scavenger can just outrun it
            if dist > 0.1 {
                dog_pos += to_p / dist * dist.min(6.0 * dt);
            }
            dog.bark -= dt;
            if dog.bark <= 0.0 {
                dog.bark = 0.8 + rng::rand() * 0.6;
                loose.sfx.write(Snd::Bark.play());
            }
            if dist < 0.8 && sc.stun <= 0.0 {
                loose.sfx.write(Snd::Bark.play());
                loose.sfx.write(Snd::Thud.at(0.5));
                sc.stun = 1.0;
                yard.time_left -= 6.0;
                let away = to_p.normalize_or_zero();
                sc.vel = away * 8.0;
                ptf.translation += away * 1.2;
                let p = ptf.translation;
                fx.dust_puff(p.with_y(0.3), 12, None, 0.2, 2.0);
                loose.pops.write(PopWord { at: p.with_y(2.0), big: true, word: "CHOMP!".into(), color: RED });
                flash.write(Flash);
                loose.spill(&mut haul, p, 1, "The dog got you! Dropped loot, lost 6 seconds");
                dog.mood = DogMood::Return;
            }
            if dog.t > 7.0 || ptf.translation.distance(dog.home) > 14.0 {
                dog.mood = DogMood::Return;
            }
        }
        DogMood::Return => {
            let to_h = (dog.home - dog_pos).with_y(0.0);
            let dh = to_h.length();
            if dh > 0.1 {
                dog_pos += to_h / dh * dh.min(3.5 * dt);
            } else {
                dog.mood = DogMood::Sleep;
                dog.t = 0.0;
            }
            // can re-trigger after a cooldown
            if dog.t > 4.0 && dist < 4.0 {
                dog.mood = DogMood::Chase;
                dog.t = 0.0;
            }
        }
    }
    let moving = dog.mood != DogMood::Sleep;
    if moving {
        let look = if dog.mood == DogMood::Chase { ptf.translation } else { dog.home };
        dtf.rotation = ry((look.x - dog_pos.x).atan2(look.z - dog_pos.z));
        bob = (t * 22.0).sin().abs() * 0.08;
    }
    dtf.translation = dog_pos.with_y(bob);
    for (leg, mut tf) in &mut legs {
        let i = leg.0;
        let ph = (i % 2) as f32 * PI + if i > 1 { 0.6 } else { 0.0 };
        tf.rotation = rx(if moving { (t * 22.0 + ph).sin() * 0.8 } else { 0.0 });
    }
}

/// The crane swings after you outside the yard, warns, then drops a car.
#[allow(clippy::too_many_arguments)]
fn swing_crane(
    mut commands: Commands,
    time: Res<Time>,
    yard: Res<Yard>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shared: Res<Mats>,
    player: Single<(Entity, &Scavenger)>,
    crane: Single<(Entity, &mut Crane)>,
    magnet: Single<Entity, With<Magnet>>,
    cable: Single<Entity, With<Cable>>,
    warning: Single<(Entity, &mut Visibility, &MeshMaterial3d<StandardMaterial>), With<CraneWarning>>,
    falling: Query<(), With<FallingCar>>,
    mut tfs: Query<&mut Transform>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();
    let (pe, sc) = *player;
    let pos = tfs.get(pe).map_or(Vec3::ZERO, |t| t.translation);
    let (ce, mut cr) = crane.into_inner();
    let target = cr.target.or(if yard.zone > 0 { Some(pos) } else { None });
    if let Some(tg) = target {
        let want = (tg.x - CRANE.x).atan2(tg.z - CRANE.z);
        cr.yaw += wrap(want - cr.yaw).clamp(-dt * 0.9, dt * 0.9);
    }
    let reach = target.map_or(10.0, |tg| tg.with_y(0.0).distance(CRANE).clamp(4.0, 14.0));
    let dir = Vec3::new(cr.yaw.sin(), 0.0, cr.yaw.cos());
    cr.magnet_pos = cr.magnet_pos.lerp((CRANE + dir * reach).with_y(6.5), ease(dt, 3.0));
    let tip = CRANE + ry(cr.yaw) * Vec3::new(0.0, 2.4 + 0.45f32.sin() * 10.0, 0.6 + 0.45f32.cos() * 10.0);
    let v = tip - cr.magnet_pos;
    let mut set = |e: Entity, f: &dyn Fn(&mut Transform)| {
        if let Ok(mut tf) = tfs.get_mut(e) {
            f(&mut tf);
        }
    };
    let (yaw, magnet_pos) = (cr.yaw, cr.magnet_pos);
    set(ce, &|tf| tf.rotation = ry(yaw));
    set(*magnet, &|tf| tf.translation = magnet_pos);
    set(*cable, &|tf| *tf = Transform { translation: magnet_pos.lerp(tip, 0.5), rotation: Quat::from_rotation_arc(Vec3::Y, v.normalize_or(Vec3::Y)), scale: Vec3::new(1.0, v.length(), 1.0) });
    let dropping = !falling.is_empty();
    if cr.carry.is_none() && !dropping {
        let g = models::car_shell(hexc(rng::pick(&["#c86a3a", "#4a7aa8", "#7a8a4a", "#d8b03a"])));
        cr.carry = Some(ment(&mut commands, &mut meshes, &shared.vc, &g, Transform::from_scale(Vec3::splat(0.6))));
    }
    if let Some(e) = cr.carry {
        set(e, &|tf| {
            tf.translation = magnet_pos - Vec3::Y;
            tf.rotation = ry(yaw);
        });
    }
    if yard.zone > 0 && cr.target.is_none() && !dropping {
        cr.t -= dt;
        if cr.t <= 0.0 {
            cr.target = Some((pos + sc.vel * 0.6).with_y(0.0));
            cr.t = 1.9; // warning time
            sfx.write(Snd::Alarm.play());
        }
    }
    let (we, mut wvis, wmat) = warning.into_inner();
    let Some(ct) = cr.target else {
        wvis.set_if_neq(Visibility::Hidden);
        return;
    };
    wvis.set_if_neq(Visibility::Visible);
    cr.t -= dt;
    let grow = 0.6 + (1.0 - cr.t.max(0.0) / 1.9) * 0.6;
    set(we, &|tf| *tf = Transform::from_translation(ct.with_y(0.1)).with_scale(Vec3::splat(grow)));
    if let Some(mut mm) = mats.get_mut(&wmat.0) {
        mm.base_color = Color::srgba(1.0, 0.23, 0.1, 0.35 + (t * 20.0).sin() * 0.2);
    }
    if rng::chance(dt * 2.0) {
        sfx.write(Snd::Alarm.play());
    }
    if cr.t <= 0.0 {
        if let Some(e) = cr.carry.take() {
            commands.entity(e).insert((FallingCar { y: 5.5, vy: 0.0, rz: 0.0, yaw }, Transform::from_translation(ct.with_y(5.5)).with_scale(Vec3::splat(0.6))));
        }
        cr.target = None;
        cr.t = 8.0 + rng::rand() * 5.0 - yard.zone as f32 * 2.0;
        sfx.write(Snd::Whoosh.play());
    }
}

/// A dropped car falls, squashes anyone under it and becomes a new pile.
#[allow(clippy::too_many_arguments)]
fn drop_car(
    time: Res<Time>,
    mut yard: ResMut<Yard>,
    mut haul: ResMut<Haul>,
    pm: Res<PileMats>,
    player: Single<(&Transform, &mut Scavenger), Without<FallingCar>>,
    car: Option<Single<(Entity, &mut Transform, &mut FallingCar)>>,
    mut fx: ResMut<Fx>,
    mut loose: Loose,
    mut flash: MessageWriter<Flash>,
) {
    let Some(car) = car else { return };
    let dt = time.delta_secs().min(0.05);
    let (e, mut tf, mut f) = car.into_inner();
    f.vy -= 40.0 * dt;
    f.y += f.vy * dt;
    f.rz += dt * 2.0;
    let fp = tf.translation.with_y(0.0);
    tf.translation.y = f.y.max(0.0);
    tf.rotation = ry(f.yaw) * models::rz(f.rz);
    if f.y > 0.0 {
        return;
    }
    loose.sfx.write(Snd::Explode.play());
    fx.dust_puff(fp.with_y(0.3), 30, Some(hexc("#c8a070")), 0.5, 6.0);
    fx.spark_burst(fp.with_y(0.4), 40, None, 8.0, 1.0);
    fx.bolt_burst(fp.with_y(0.4), 14, None);
    fx.add_shake(0.8);
    let (ptf, mut sc) = player.into_inner();
    let d = ptf.translation.with_y(0.0).distance(fp);
    if d < 1.7 {
        sc.stun = 2.0;
        yard.time_left -= 10.0;
        loose.pops.write(PopWord { at: ptf.translation.with_y(2.0), big: true, word: "SQUASHED!".into(), color: RED });
        flash.write(Flash);
        loose.spill(&mut haul, ptf.translation, 2, "Flattened by the crane! Lost 10 seconds and spilled loot");
    } else if d < 3.0 {
        loose.pops.write(PopWord { at: fp.with_y(2.0), big: false, word: "phew".into(), color: CREAM });
    }
    // the dropped car becomes a fresh pile worth digging
    loose.commands.entity(e).despawn();
    if yard.crane_piles < 3 {
        yard.crane_piles += 1;
        let Loose { commands, meshes, shared, .. } = &mut loose;
        add_pile(commands, meshes, shared, &pm, fp, 1.2, 0);
    }
}

/// Bank the sack at the van; ENTER there leaves early.
#[allow(clippy::too_many_arguments)]
fn bank_at_van(
    keys: Res<ButtonInput<KeyCode>>,
    mut yard: ResMut<Yard>,
    mut haul: ResMut<Haul>,
    player: Single<&Transform, With<Scavenger>>,
    mut whistle: ResMut<NextState<Whistle>>,
    mut toasts: MessageWriter<Toast>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dv = player.translation.distance(VAN);
    if dv >= 2.6 {
        return;
    }
    if !haul.sack.is_empty() {
        let n = haul.sack.len();
        let sack = std::mem::take(&mut haul.sack);
        haul.banked.extend(sack);
        sfx.write(Snd::Ding.pitched(1.2));
        sfx.write(Snd::Ding.pitched(1.5));
        toasts.write(Toast(format!("Banked {n} item{} in the van", if n > 1 { "s" } else { "" }), TEAL));
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        yard.early = true;
        blow_whistle(&mut haul, &mut whistle, &mut sfx);
    }
}

fn spin_glints(time: Res<Time>, mut q: Query<(&mut Transform, &Glint, &Visibility)>) {
    let t = time.elapsed_secs();
    for (mut tf, g, vis) in &mut q {
        if vis == Visibility::Hidden {
            continue;
        }
        tf.rotation = ry(t * 2.0);
        tf.translation.y = 1.4 + g.zone as f32 * 0.35 + (t * 3.0 + tf.translation.x).sin() * 0.12;
    }
}

/// Swing round to face away from the van; hold still close to it so the
/// view doesn't spin while you unload.
fn follow_camera(time: Res<Time>, whistle: Option<Res<State<Whistle>>>, fx: Res<Fx>, player: Single<(&Transform, &Scavenger), Without<YardCam>>, cam: Single<(&mut Transform, &mut YardCam)>) {
    let dt = time.delta_secs().min(0.05);
    let (ptf, sc) = *player;
    let pos = ptf.translation;
    let (mut tf, mut c) = cam.into_inner();
    let r = pos.length();
    if r > 2.5 && whistle.is_some_and(|w| *w.get() == Whistle::Open) {
        let d = wrap(pos.x.atan2(pos.z) - c.yaw);
        c.yaw += d * ease(dt, 1.6 * ((r - 2.5) / 3.0).min(1.0));
    }
    c.target = c.target.lerp(pos + sc.vel * 0.15, ease(dt, 6.0));
    let at = c.target;
    let p = Vec3::new(at.x - c.yaw.sin() * 7.2, 9.5, at.z - c.yaw.cos() * 7.2);
    *tf = Transform::from_translation(p).looking_at(Vec3::new(at.x, 0.4, at.z), Vec3::Y);
    tf.translation += fx.shake_offset(time.elapsed_secs());
}

// ------------------------------------------------------------------ UI
fn show_dig_bar(
    yard: Res<Yard>,
    player: Single<&Transform, With<Scavenger>>,
    piles: Query<&Pile>,
    cam: Single<(&Camera, &GlobalTransform), With<YardCam>>,
    bar: Single<&mut Node, (With<DigBar>, Without<DigFill>)>,
    fill: Single<&mut Node, (With<DigFill>, Without<DigBar>)>,
) {
    let (camera, gt) = *cam;
    let dig = yard
        .digging
        .and_then(|(e, t)| piles.get(e).ok().map(|p| t / DIG_TIME[p.zone]))
        .and_then(|f| camera.world_to_viewport(gt, player.translation.with_y(2.3)).ok().map(|px| (px, f)));
    let (mut bar, mut fill) = (bar.into_inner(), fill.into_inner());
    bar.display = if dig.is_some() { Display::Flex } else { Display::None };
    if let Some((px, f)) = dig {
        bar.left = Val::Px(px.x - 40.0);
        bar.top = Val::Px(px.y);
        fill.width = Val::Percent(f.min(1.0) * 100.0);
    }
}

/// Name and dig time of the nearest pile.
fn show_pile_tag(
    yard: Res<Yard>,
    player: Single<&Transform, With<Scavenger>>,
    piles: Query<(&Transform, &Pile), Without<Scavenger>>,
    cam: Single<(&Camera, &GlobalTransform), With<YardCam>>,
    tag: Single<&mut Node, With<TagNode>>,
    mut text: Single<&mut Text, With<TagText>>,
) {
    let pos = player.translation;
    let mut best = 2.2;
    let mut found = None;
    for (tf, p) in &piles {
        let d = tf.translation.distance(pos) - p.r;
        if !p.dug && d < best {
            best = d;
            let hint = if d < 1.0 { "hold E".to_string() } else { format!("{}s dig", DIG_TIME[p.zone]) };
            found = Some((tf.translation.with_y(1.9 + p.zone as f32 * 0.35), format!("{} · {hint}", PILE_NAMES[p.zone])));
        }
    }
    let (camera, gt) = *cam;
    let shown = found.and_then(|(at, msg)| camera.world_to_viewport(gt, at).ok().map(|px| (px, msg))).filter(|_| yard.digging.is_none());
    let mut node = tag.into_inner();
    node.display = if shown.is_some() { Display::Flex } else { Display::None };
    if let Some((px, msg)) = shown {
        node.left = Val::Px(px.x - 120.0);
        node.top = Val::Px(px.y - 14.0);
        if text.0 != msg {
            text.0 = msg;
        }
    }
}

/// Timer, zone, sack, repairs needed, van.
fn refresh_hud(mut commands: Commands, yard: Res<Yard>, haul: Res<Haul>, s: Res<Run>, fonts: Res<Fonts>, hud: Single<Entity, With<HudCol>>) {
    let fonts = &*fonts;
    let tl = yard.time_left.max(0.0).ceil() as i32;
    let (mut plate, mut bolts) = (0, 0);
    for p in &s.design.parts {
        let (res, n) = state::repair_cost(p.kind, p.dmg);
        if res == Stock::Plate { plate += n } else { bolts += n }
    }
    let all = haul.banked.iter().chain(haul.sack.iter());
    let (np, nb) = all.fold((0, 0), |a, l| (a.0 + (*l == Loot::Plate) as i32, a.1 + (*l == Loot::Bolts) as i32));
    let (hp, hb) = (s.plate + np * 2, s.bolts + nb * 2);
    let zone = yard.zone;
    commands.entity(*hud).despawn_related::<Children>().with_children(|col| {
        col.spawn((
            widget::panel(Node { justify_content: JustifyContent::Center, ..default() }),
            Pickable::IGNORE,
            children![(widget::title_text(fonts, format!("{tl}s"), 44.0, if tl <= 10 { RED } else { CREAM }), Pickable::IGNORE)],
        ));
        col.spawn((
            widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() }),
            Pickable::IGNORE,
            children![
                (widget::title_text(fonts, ZONE_NAMES[zone].to_uppercase(), 16.0, hex(ZONE_HEX[zone])), Pickable::IGNORE),
                (widget::text(fonts, ZONE_DESC[zone], 13.0, Color::srgb_u8(0xd9, 0xcc, 0xb4)), Pickable::IGNORE),
            ],
        ));
        col.spawn((widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(5.0), ..default() }), Pickable::IGNORE)).with_children(|p| {
            p.spawn((widget::title_text(fonts, format!("SACK {}/{SACK_MAX}", haul.sack.len()), 15.0, TEAL), Pickable::IGNORE));
            chips(p, fonts, &haul.sack);
        });
        let ok = |h: i32, n: i32| if h >= n { GREEN } else { hex("#ee7a32") };
        col.spawn((
            widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), ..default() }),
            Pickable::IGNORE,
            children![
                (widget::title_text(fonts, "REPAIRS NEED", 15.0, TEAL), Pickable::IGNORE),
                (widget::text(fonts, format!("plate {plate} (have {hp})"), 14.0, ok(hp, plate)), Pickable::IGNORE),
                (widget::text(fonts, format!("bolts {bolts} (have {hb})"), 14.0, ok(hb, bolts)), Pickable::IGNORE),
            ],
        ));
        col.spawn((widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(5.0), ..default() }), Pickable::IGNORE)).with_children(|p| {
            p.spawn((widget::title_text(fonts, "IN THE VAN", 15.0, TEAL), Pickable::IGNORE));
            chips(p, fonts, &haul.banked);
        });
    });
}

fn show_end_card(
    mut commands: Commands,
    yard: Res<Yard>,
    haul: Res<Haul>,
    fonts: Res<Fonts>,
    hud: Single<Entity, With<HudCol>>,
    mut overlays: Query<&mut Node, Or<(With<DigBar>, With<TagNode>)>>,
) {
    let fonts = &*fonts;
    commands.entity(*hud).despawn_related::<Children>();
    for mut n in &mut overlays {
        n.display = Display::None;
    }
    commands
        .spawn((
            widget::ui_root("End card", Node { height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }),
            Pickable::default(),
            GlobalZIndex(60),
            DespawnOnExit(Screen::Scrap),
        ))
        .with_children(|w| {
            w.spawn(widget::panel(Node { width: Val::Px(520.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(12.0), padding: UiRect::all(Val::Px(22.0)), ..default() })).with_children(|c| {
                c.spawn(widget::title_text(fonts, if yard.early { "Heading back to the workshop" } else { "Whistle! Yard's closed." }, 32.0, YELLOW));
                c.spawn(widget::title_text(fonts, "BROUGHT HOME", 15.0, TEAL));
                chips(c, fonts, &haul.banked);
                if !haul.lost.is_empty() {
                    c.spawn(widget::title_text(fonts, "CONFISCATED AT THE GATE (STILL IN YOUR SACK)", 15.0, hex("#ee7a32")));
                    chips(c, fonts, &haul.lost);
                }
                c.spawn(widget::button(fonts, "To the workshop", BtnStyle::Primary, 26.0)).observe(go_home);
            });
        });
}

/// Unload the van into your stash.
fn go_home(_: On<Activate>, mut yard: ResMut<Yard>, haul: Res<Haul>, mut s: ResMut<Run>, mut next: ResMut<NextState<Screen>>) {
    if yard.done {
        return;
    }
    yard.done = true;
    for l in &haul.banked {
        match l {
            Loot::Plate => s.plate += 2,
            Loot::Bolts => s.bolts += 2,
            Loot::Part(k) => {
                s.add_item(*k, 0.0);
            }
        }
    }
    next.set(Screen::Workshop);
}

fn chips(c: &mut ChildSpawnerCommands, fonts: &Fonts, l: &[Loot]) {
    c.spawn((Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), ..default() }, Pickable::IGNORE)).with_children(|w| {
        if l.is_empty() {
            w.spawn((widget::text(fonts, "empty", 13.0, Color::srgba(1.0, 0.95, 0.86, 0.5)), Pickable::IGNORE));
        }
        for x in l {
            w.spawn((Node { padding: UiRect::axes(Val::Px(7.0), Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(6.0)), ..default() }, BackgroundColor(hex(RARITY_HEX[x.rarity()])), Pickable::IGNORE)).with_children(|t| {
                t.spawn((widget::text(fonts, x.name(), 13.0, CREAM), Pickable::IGNORE));
            });
        }
    });
}

