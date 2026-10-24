//! Arena fight scene: fixed-step physics, collision damage, the match rules,
//! the follow camera and the HUD.
use crate::ai::{Brain, Hazard};
use crate::arena::{self, Crowd, PitLight, A, PIT};
use crate::audio;
use crate::fx::Fx;
use crate::models::{self, Mats};
use crate::parts::{Category, Design, PartKind};
use crate::physics::{Phys, STEP};
use crate::robot::{Boost, Debris, DetachCtx, Ctx, Ev, Input, PartOf, Robot, Spawn};
use crate::scene::{self, SunOpts};
use crate::state::{self, BattleResult, GameState};
use crate::ui::{self, Fonts, PointerOverUi};
use crate::{param, Scoped, Screen};
use bevy::prelude::*;
use rapier3d::prelude::ColliderHandle;
use std::collections::{HashMap, HashSet};

const MATCH_TIME: f32 = 120.0;

pub struct BattlePlugin;
impl Plugin for BattlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Fight), enter)
            .add_systems(OnExit(Screen::Fight), exit)
            .add_systems(Update, (update, hud, popwords, flash_fade).chain().run_if(in_state(Screen::Fight)));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Intro,
    Fight,
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PairKey {
    Bots,
    Pair(ColliderHandle, ColliderHandle),
}

#[derive(Resource)]
pub struct Battle {
    pub phys: Phys,
    pub fx: Fx,
    pub part_of: PartOf,
    pub bots: Vec<Robot>,
    pub brain: Brain,
    pub auto: Option<Brain>,
    pub debris: Vec<Debris>,
    pub test: bool,
    pub enemy_design: Design,
    acc: f32,
    freeze: f32,
    slowmo: f32,
    slowmo_t: f32,
    pub time: f32,
    pub phase: Phase,
    phase_t: f32,
    pub time_left: f32,
    immob: [f32; 2],
    // last place each bot was seen really moving, and how long since
    still: [Option<(Vec3, f32, f32)>; 2],
    pair_cd: HashMap<PairKey, f32>,
    pre_vel: [Vec3; 2],
    pub excitement: f32,
    cam_target: Vec3,
    cam_dist: f32,
    pub result: Option<BattleResult>,
    countdown: i32,
    player_lost: Vec<usize>,
    respawn_t: f32,
    walls: HashSet<ColliderHandle>,
    ev: Vec<Ev>,
    glow: [Handle<StandardMaterial>; 4],
    pops: Vec<(Vec3, bool, Option<String>, Option<Color>)>,
    banner: Option<(String, String)>,
    flash: bool,
    hud_t: i32,
    /// headless-ish benchmark: run the sim as fast as possible and log the result
    pub bench: bool,
}

#[derive(Component)]
struct FightCam;
#[derive(Component)]
struct HpFill(usize);
#[derive(Component)]
struct CoreText(usize);
#[derive(Component)]
struct Pips(usize);
#[derive(Component)]
struct BoostFill(usize);
#[derive(Component)]
struct TimerText;
#[derive(Component)]
struct ImmobText;
#[derive(Component)]
struct BannerNode(f32);
#[derive(Component)]
struct PopWord {
    t: f32,
    at: Vec2,
}
#[derive(Component)]
struct Flash(f32);

const WORDS_BIG: [&str; 6] = ["CRUNCH!", "KA-BLAM!", "WHAM!", "CLANG!", "SMASH!", "KERRANG!"];
const WORDS_SMALL: [&str; 4] = ["bonk", "tink", "clonk", "dink"];

fn enter(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shared: Res<Mats>,
    s: Res<GameState>,
    fonts: Res<Fonts>,
) {
    let opps = state::opponents();
    let test = param("test").is_some() || s.testing;
    let oi = param("opp").and_then(|v| v.parse::<usize>().ok()).unwrap_or(s.round).min(opps.len() - 1);
    let (enemy, skill) = if test { (state::dummy(), 0.0) } else { (opps[oi].design.clone(), opps[oi].skill) };
    let mut phys = Phys::new();
    scene::sun_lights(&mut commands, SunOpts { size: 14.0, dir: Vec3::new(-10.0, 9.0, -7.0), ..default() }, Scoped);
    models::reseed(7);
    scene::sky_and_clouds(&mut commands, &mut meshes, &mut mats, Color::srgb_u8(0x4f, 0x9f, 0xe0), Color::srgb_u8(0xcf, 0xe9, 0xf7), 10, 60.0, Scoped);
    let info = arena::build_arena(&mut commands, &mut meshes, &mut mats, &shared, &mut phys, Scoped);
    let fx = Fx::new(&mut commands, &mut meshes, &mut mats, Scoped);
    let glow = [
        models::glow_mat(&mut mats, models::hexc("#ffd25a"), 2.4),
        models::glow_mat(&mut mats, models::hexc("#ff6a3c"), 2.4),
        models::glow_mat(&mut mats, models::hexc("#3a3335"), 1.0),
        models::glow_mat(&mut mats, models::hexc("#ff5a3c"), 2.4),
    ];
    let mut part_of = PartOf::new();
    let mut sp = Spawn { commands: &mut commands, meshes: &mut meshes, mats: &mut mats, shared: &shared, glow: glow.clone() };
    let player = Robot::new(&mut sp, &mut phys, &mut part_of, &s.design, Vec3::new(0.0, 0.0, -6.0), 0.0, 0, Scoped);
    let enemy_bot = Robot::new(&mut sp, &mut phys, &mut part_of, &enemy, Vec3::new(0.0, 0.0, 6.0), std::f32::consts::PI, 1, Scoped);
    let hz = vec![Hazard { x: PIT.x, z: PIT.z, r: PIT.h }];
    let auto = param("auto").map(|_| Brain::new(0.7, hz.clone()));
    let bench = param("bench").is_some();
    let mut fog_cam = commands.spawn((scene::camera_bundle(40.0, Transform::from_xyz(0.0, 14.0, -14.0).looking_at(Vec3::ZERO, Vec3::Y)), FightCam, Scoped));
    // haze pushes the stands back
    fog_cam.insert(DistanceFog { color: Color::srgb_u8(0xc9, 0xd9, 0xef), falloff: FogFalloff::Linear { start: 18.0, end: 70.0 }, ..default() });
    audio::crowd(true);
    build_hud(&mut commands, &fonts, test);
    commands.insert_resource(Battle {
        phys,
        fx,
        part_of,
        bots: vec![player, enemy_bot],
        brain: Brain::new(skill, hz),
        auto,
        debris: vec![],
        test,
        enemy_design: enemy,
        acc: 0.0,
        freeze: 0.0,
        slowmo: 1.0,
        slowmo_t: 0.0,
        time: 0.0,
        phase: if test || bench { Phase::Fight } else { Phase::Intro },
        phase_t: 0.0,
        time_left: if test { f32::INFINITY } else { MATCH_TIME },
        immob: [0.0; 2],
        still: [None, None],
        pair_cd: HashMap::new(),
        pre_vel: [Vec3::ZERO; 2],
        excitement: 0.0,
        cam_target: Vec3::ZERO,
        cam_dist: 14.0,
        result: None,
        countdown: -1,
        player_lost: vec![],
        respawn_t: 0.0,
        walls: info.walls,
        ev: vec![],
        glow,
        pops: vec![],
        banner: None,
        flash: false,
        hud_t: 0,
        bench,
    });
}

fn exit(mut commands: Commands, mut b: ResMut<Battle>) {
    audio::crowd(false);
    for r in &b.bots {
        r.motor.stop();
        if let Some(h) = &r.spin_hum {
            h.stop();
        }
    }
    b.bots.clear();
    commands.remove_resource::<Battle>();
}

impl Battle {
    fn hit_stop(&mut self, s: f32) {
        self.freeze = self.freeze.max(s);
    }
    fn pop(&mut self, at: Vec3, big: bool, word: Option<&str>, color: Option<Color>) {
        self.pops.push((at, big, word.map(|w| w.to_string()), color));
    }
    fn set_banner(&mut self, a: &str, b: &str) {
        self.banner = Some((a.into(), b.into()));
    }

    fn part_world_pos(&self, (t, i): (usize, usize)) -> Vec3 {
        let r = &self.bots[t];
        r.part_world(&r.parts[i], Vec3::ZERO)
    }

    /// Apply damage to a part (may knock it off or kill the bot).
    fn damage(&mut self, commands: &mut Commands, shared: &Mats, (t, i): (usize, usize), dmg: f32, dir: Option<Vec3>) {
        let mut detached = vec![];
        let Battle { phys, part_of, fx, debris, bots, time, .. } = self;
        let mut dc = DetachCtx { phys, part_of, fx, commands, shared, debris, detached: &mut detached, hit_stop: 0.0, time: *time };
        bots[t].damage(&mut dc, i, dmg, dir);
        let hs = dc.hit_stop;
        self.hit_stop(hs);
        for d in detached {
            self.on_detach(d);
        }
        // keep the debris count bounded
        while self.debris.len() > 60 {
            let d = self.debris.remove(0);
            self.phys.w.remove_body_with_colliders(d.body, true);
            commands.entity(d.ent).try_despawn();
        }
    }

    fn on_hit(&mut self, commands: &mut Commands, shared: &Mats, att: Option<usize>, tgt: (usize, usize), mut dmg: f32, pt: Vec3, big: bool, word: Option<&'static str>) {
        if self.phase == Phase::Over && !self.test {
            dmg *= 0.3;
        }
        let dir = att.map(|a| (self.bots[tgt.0].pos - self.bots[a].pos).with_y(0.3).normalize_or_zero());
        self.damage(commands, shared, tgt, dmg, dir);
        // the juice: chunks of the victim's paint and plate fly off, more for bigger hits
        if dmg > 3.0 {
            let n = ((2.0 + dmg * 0.45) as usize).min(26);
            let paint = self.bots[tgt.0].paint;
            self.fx.shard_burst(pt, n, paint, dir, 3.0 + (dmg / 8.0).min(5.0));
            if dmg > 15.0 {
                self.fx.bolt_burst(pt, (dmg / 8.0) as usize, dir);
            }
        }
        if let Some(a) = att {
            if a != tgt.0 {
                self.bots[a].damage_dealt += dmg;
            }
        }
        let i = (dmg / 40.0).min(1.0);
        self.excitement = (self.excitement + i * 0.8).min(1.5);
        audio::cheer(i);
        let green = Some(Color::srgb_u8(0x7c, 0xcf, 0x5a));
        let at = pt + Vec3::Y * 0.6;
        if big {
            self.pop(at, true, word, if word.is_some() { green } else { None });
        } else if word.is_some() {
            self.pop(at, false, word, green);
        } else if dmg > 6.0 && crate::rng::chance(0.4) {
            self.pop(pt + Vec3::Y * 0.5, false, None, None);
        }
    }

    fn on_detach(&mut self, (t, i): (usize, usize)) {
        self.excitement = (self.excitement + 0.4).min(1.5);
        audio::cheer(0.6);
        // parts blown off by the final KO explosion or after the bell aren't "lost"
        if t == 0 && self.bots[0].alive && self.phase != Phase::Over {
            self.player_lost.push(i);
        }
        if crate::rng::chance(0.6) {
            let p = &self.bots[t].parts[i];
            let w = match p.def().category {
                Category::Drive => if p.kind == PartKind::Leg { "LEG OFF!" } else { "WHEEL OFF!" },
                Category::Weapon => "DISARMED!",
                _ if p.kind == PartKind::Armour => "PLATE OFF!",
                _ => "BITS!",
            };
            let at = self.bots[t].pos + Vec3::Y;
            self.pop(at, true, Some(w), Some(Color::srgb_u8(0xff, 0x8a, 0x5c)));
        }
    }

    fn fixed_step(&mut self, commands: &mut Commands, shared: &Mats, keys: &PlayerKeys) {
        // input
        let fighting = self.phase == Phase::Fight;
        self.bots[0].input = if fighting && self.auto.is_some() {
            let (a, b) = self.bots.split_at(1);
            self.auto.as_mut().unwrap().think(&a[0], &b[0], &self.phys, STEP, self.time)
        } else if fighting && self.bots[0].alive {
            Input {
                throttle: keys.fwd,
                turn: keys.turn,
                fire: false,
                fire_slots: Some(keys.fire),
                brake: keys.brake,
                boost: keys.boost,
            }
        } else {
            Input::default()
        };
        self.bots[1].input = if fighting && !self.test {
            let (a, b) = self.bots.split_at(1);
            self.brain.think(&b[0], &a[0], &self.phys, STEP, self.time)
        } else {
            Input::default()
        };
        {
            let Battle { phys, fx, part_of, ev, bots, time, .. } = self;
            let mut ctx = Ctx { phys, fx, part_of, ev, time: *time };
            for r in bots.iter_mut() {
                r.step(&mut ctx, STEP);
            }
        }
        // closing speeds are judged on pre-impact velocities (after the step the
        // solver has already soaked up the ram)
        for k in 0..2 {
            self.pre_vel[k] = self.bots[k].linvel(&self.phys);
        }
        let forces = self.phys.step();
        for e in std::mem::take(&mut self.ev) {
            match e {
                Ev::HitStop(s) => self.hit_stop(s),
                Ev::Hit { att, tgt, dmg, pt, big, word } => self.on_hit(commands, shared, att, tgt, dmg, pt, big, word),
            }
        }
        for f in forces {
            self.on_force(commands, shared, f.collider1, f.collider2, f.total_force_magnitude);
        }
        self.pair_cd.retain(|_, v| {
            *v -= STEP;
            *v > 0.0
        });
        if self.phase == Phase::Fight {
            self.time_left -= STEP;
            self.check_end();
        }
    }

    fn on_force(&mut self, commands: &mut Commands, shared: &Mats, h1: ColliderHandle, h2: ColliderHandle, f: f32) {
        let p1 = self.part_of.get(&h1).copied();
        let p2 = self.part_of.get(&h2).copied();
        // bot-vs-bot hits share one cooldown so a wide front can't multi-hit
        let key = match (p1, p2) {
            (Some(a), Some(b)) if a.0 != b.0 => PairKey::Bots,
            _ => if h1.into_raw_parts() <= h2.into_raw_parts() { PairKey::Pair(h1, h2) } else { PairKey::Pair(h2, h1) },
        };
        if self.pair_cd.contains_key(&key) {
            return;
        }
        let kind = |s: &Self, p: (usize, usize)| s.bots[p.0].parts[p.1].kind;
        let mult = |k: PartKind| match k {
            PartKind::Spike => 2.4,
            PartKind::Wedge => 0.5,
            _ => 1.0,
        };
        let resist = |k: PartKind| match k {
            PartKind::Spike | PartKind::Wedge => 0.35,
            PartKind::Armour => 0.75,
            _ => 1.0,
        };
        match (p1, p2) {
            (Some(p1), Some(p2)) if p1.0 != p2.0 => {
                // a spike that meets the foe point-first does damage by closing speed:
                // ~6 hp per m/s (capped at 40), so a full-speed charge lands a ~40 hp stab
                let stab = |s: &Self, a: (usize, usize), o: (usize, usize)| {
                    if kind(s, a) != PartKind::Spike {
                        return 0.0;
                    }
                    let r = &s.bots[a.0];
                    let dir = r.dir_world(&r.parts[a.1], Vec3::Z);
                    let closing = (s.pre_vel[a.0] - s.pre_vel[o.0]).dot(dir);
                    if closing < 1.2 { 0.0 } else { (6.0 * closing).min(40.0) }
                };
                let (s1, s2) = (stab(self, p1, p2), stab(self, p2, p1));
                if f < if s1 > 0.0 || s2 > 0.0 { 600.0 } else { 2600.0 } {
                    return;
                }
                self.pair_cd.insert(key, 0.22);
                let i = ((f - 2600.0) / 9000.0).min(1.0).max((s1.max(s2) / 40.0).min(1.0));
                let base = 3.0 + 15.0 * ((f - 2600.0) / 9000.0).clamp(0.0, 1.0);
                let (a, b) = (self.part_world_pos(p1), self.part_world_pos(p2));
                let mid = a.lerp(b, 0.5);
                // hits to the side or rear land harder than meeting their front
                let flank = |s: &Self, att: usize, tgt: usize| {
                    let from = (s.bots[att].pos - s.bots[tgt].pos).with_y(0.0).normalize_or_zero();
                    s.bots[tgt].fwd.with_y(0.0).normalize_or_zero().dot(from) < 0.25
                };
                let (f12, f21) = (flank(self, p1.0, p2.0), flank(self, p2.0, p1.0));
                let d12 = (base * mult(kind(self, p1))).max(s1) * resist(kind(self, p2)) * if f12 { 1.4 } else { 1.0 };
                let d21 = (base * mult(kind(self, p2))).max(s2) * resist(kind(self, p1)) * if f21 { 1.4 } else { 1.0 };
                self.on_hit(commands, shared, Some(p1.0), p2, d12, mid, i > 0.55 && d12 >= d21, if f12 && d12 > 10.0 { Some("FLANKED!") } else { None });
                self.on_hit(commands, shared, Some(p2.0), p1, d21, mid, i > 0.55 && d21 > d12, if f21 && d21 > 10.0 { Some("FLANKED!") } else { None });
                let n = (b - a).normalize_or_zero();
                self.fx.spark_burst(mid, (8.0 + 30.0 * i) as usize, Some(n.cross(Vec3::Y)), 6.0, 1.0);
                self.fx.bolt_burst(mid, (i * 6.0) as usize, None);
                audio::thud(0.3 + i * 0.7);
                self.fx.add_shake(0.15 + i * 0.45);
                if i > 0.3 {
                    self.hit_stop(0.03 + i * 0.05);
                }
            }
            (Some(_), Some(_)) => {}
            (p1, p2) => {
                let (p, other) = match (p1, p2) {
                    (Some(p), _) => (p, h2),
                    (_, Some(p)) => (p, h1),
                    _ => return,
                };
                if !self.bots[p.0].parts[p.1].attached {
                    return;
                }
                let k = kind(self, p);
                if self.walls.contains(&other) {
                    if f < 4500.0 {
                        return;
                    }
                    self.pair_cd.insert(key, 0.3);
                    let i = ((f - 4500.0) / 10000.0).min(1.0);
                    let pos = self.part_world_pos(p);
                    self.damage(commands, shared, p, 6.0 * i * resist(k), None);
                    self.fx.spark_burst(pos, (6.0 + 20.0 * i) as usize, None, 5.0, 1.0);
                    self.fx.dust(pos.with_y(0.1), 4);
                    audio::thud(0.25 + i * 0.6);
                    self.fx.add_shake(0.1 + i * 0.3);
                    if i > 0.4 {
                        self.pop(pos.with_y(1.0), false, Some("BONK"), None);
                    }
                } else if f > 5000.0 {
                    // floor slam after being thrown
                    self.pair_cd.insert(key, 0.4);
                    let i = ((f - 5000.0) / 12000.0).min(1.0);
                    let pos = self.part_world_pos(p);
                    if i > 0.15 {
                        self.damage(commands, shared, p, 5.0 * i, None);
                        self.fx.dust(pos.with_y(0.05), (4.0 + i * 10.0) as usize);
                        audio::thud(0.3 + i * 0.6);
                        self.fx.add_shake(0.1 + i * 0.4);
                    }
                }
            }
        }
    }

    fn check_end(&mut self) {
        if self.test {
            return;
        }
        for k in 0..2 {
            let foe = 1 - k;
            let b = &self.bots[k];
            if !b.alive {
                return self.finish(foe, "KNOCKOUT!");
            }
            if b.pos.y < -0.7 && arena::in_pit(b.pos.x, b.pos.z) {
                return self.finish(foe, "PITTED!");
            }
            if b.pos.y < -0.7 {
                return self.finish(foe, "OUT OF THE ARENA!");
            }
            let drives = b.parts.iter().any(|p| p.attached && p.def().category == Category::Drive);
            let stuck = !drives || b.grounded_frac == 0.0;
            // ...or simply not getting anywhere (upside down and scrabbling, wedged
            // on the foe, pinned): no real movement or turning resets the clock
            let yaw = b.fwd.x.atan2(b.fwd.z);
            let st = self.still[k].get_or_insert((b.pos, yaw, 0.0));
            let dy = (yaw - st.1).sin().atan2((yaw - st.1).cos()).abs();
            if b.pos.distance(st.0) > 0.35 || (b.up.y > 0.5 && dy > 0.5) {
                *st = (b.pos, yaw, 0.0);
            } else {
                st.2 += STEP;
            }
            let t = (if stuck { self.immob[k] + STEP } else { 0.0 }).max(st.2);
            self.immob[k] = t;
            if t > 10.0 {
                return self.finish(foe, "IMMOBILISED!");
            }
        }
        if self.time_left <= 0.0 {
            let ps = self.bots[0].damage_dealt - self.bots[0].damage_taken * 0.5;
            let es = self.bots[1].damage_dealt - self.bots[1].damage_taken * 0.5;
            self.finish(if ps >= es { 0 } else { 1 }, "TIME! JUDGES' DECISION");
        }
    }

    fn finish(&mut self, winner: usize, how: &str) {
        if self.phase == Phase::Over {
            return;
        }
        self.phase = Phase::Over;
        self.phase_t = 0.0;
        self.slowmo = 0.3;
        self.slowmo_t = 1.4;
        let won = winner == 0;
        self.bots[0].halted = true;
        self.bots[1].halted = true;
        self.flash = true;
        audio::buzzer();
        audio::cheer(1.5);
        self.excitement = 1.5;
        let sub = format!("{} wins!", self.bots[winner].label);
        self.set_banner(how, &sub);
        // trophy: one of the enemy's surviving (or fallen) parts
        let mut trophy = None;
        if won {
            let pool: Vec<PartKind> = self.bots[1].parts.iter().map(|p| p.kind).filter(|k| !matches!(k, PartKind::Core | PartKind::Block)).collect();
            let weapons: Vec<PartKind> = pool.iter().copied().filter(|k| k.def().category == Category::Weapon).collect();
            let pick = if !weapons.is_empty() && crate::rng::chance(0.7) { &weapons } else { &pool };
            if !pick.is_empty() {
                trophy = Some(crate::rng::pick(pick));
            }
        }
        let p = &self.bots[0];
        self.result = Some(BattleResult {
            won,
            how: how.into(),
            time: MATCH_TIME - self.time_left,
            time_left: self.time_left.max(0.0),
            dmg_dealt: p.damage_dealt.round(),
            dmg_taken: p.damage_taken.round(),
            lost: self.player_lost.iter().map(|&i| p.parts[i].placed.id).collect(),
            damaged: p.parts.iter().enumerate().filter(|(i, _)| !self.player_lost.contains(i)).map(|(_, q)| (q.placed.id, (q.hp / q.max_hp).max(0.1))).collect(),
            trophy,
            opponent: self.bots[1].label.clone(),
        });
        if self.bench {
            info!("[bench] {{\"won\":{},\"how\":\"{}\",\"t\":{:.0},\"dealt\":{:.0},\"taken\":{:.0}}}", won, how, MATCH_TIME - self.time_left, p.damage_dealt, p.damage_taken);
        }
    }
}

#[derive(Default)]
struct PlayerKeys {
    fwd: f32,
    turn: f32,
    fire: [bool; 2],
    brake: bool,
    boost: bool,
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    mut b: ResMut<Battle>,
    shared: Res<Mats>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    over_ui: Res<PointerOverUi>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut tfs: Query<&mut Transform, Without<FightCam>>,
    mut matq: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut cam: Query<&mut Transform, With<FightCam>>,
    mut crowd: Query<&mut Crowd>,
    mut pit: Query<&mut PointLight, With<PitLight>>,
    mut s: ResMut<GameState>,
    mut next: ResMut<NextState<Screen>>,
) {
    let dt = time.delta_secs().min(0.05);
    let b = &mut *b;
    b.time += dt;
    b.phase_t += dt;
    if b.test && keys.just_pressed(KeyCode::Escape) {
        s.testing = false;
        next.set(Screen::Workshop);
        return;
    }
    if b.phase == Phase::Intro {
        let steps = ["3", "2", "1", "ACTIVATE!"];
        let st = (b.phase_t / 0.8).floor() as i32 - 1;
        if st != b.countdown && st >= 0 && st < 4 {
            b.countdown = st;
            b.set_banner(steps[st as usize], if st == 3 { "WASD walk · CLICK fire · SPACE handbrake · SHIFT boost" } else { "" });
            if st == 3 {
                audio::buzzer();
                audio::cheer(1.0);
                b.phase = Phase::Fight;
                b.phase_t = 0.0;
            } else {
                audio::bell();
            }
        }
    }
    let k = |c: &[KeyCode]| c.iter().any(|x| keys.pressed(*x));
    let free_mouse = !over_ui.0;
    let pk = PlayerKeys {
        fwd: (k(&[KeyCode::KeyW, KeyCode::ArrowUp]) as i32 - k(&[KeyCode::KeyS, KeyCode::ArrowDown]) as i32) as f32,
        turn: (k(&[KeyCode::KeyD, KeyCode::ArrowRight]) as i32 - k(&[KeyCode::KeyA, KeyCode::ArrowLeft]) as i32) as f32,
        fire: [
            (free_mouse && mouse.pressed(MouseButton::Left)) || k(&[KeyCode::KeyF, KeyCode::KeyJ]),
            (free_mouse && mouse.pressed(MouseButton::Right)) || k(&[KeyCode::KeyG, KeyCode::KeyL]),
        ],
        brake: k(&[KeyCode::Space]),
        boost: k(&[KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::KeyK]),
    };
    // slow motion after the bell
    if b.slowmo_t > 0.0 {
        b.slowmo_t -= dt;
        if b.slowmo_t <= 0.0 {
            b.slowmo = 1.0;
        }
    }
    let mut pdt = dt * b.slowmo;
    if b.freeze > 0.0 {
        b.freeze -= dt;
        pdt = 0.0;
    }
    if b.bench {
        // as fast as we can (~250ms of stepping per frame), no hit-stop
        let t0 = bevy::platform::time::Instant::now();
        while t0.elapsed().as_secs_f32() < 0.25 {
            if b.phase == Phase::Over {
                break;
            }
            b.fixed_step(&mut commands, &shared, &pk);
            b.time += STEP;
        }
        b.freeze = 0.0;
    } else {
        b.acc += pdt;
        let mut steps = 0;
        while b.acc >= STEP && steps < 4 {
            b.fixed_step(&mut commands, &shared, &pk);
            b.acc -= STEP;
            steps += 1;
        }
        if steps >= 4 {
            b.acc = 0.0;
        }
    }
    // visuals
    let vdt = if pdt > 0.0 { dt } else { 0.0 };
    let mut out = vec![];
    let mut eye = vec![];
    {
        let Battle { bots, phys, fx, time, .. } = b;
        for r in bots.iter_mut() {
            r.sync_visual(phys, fx, vdt, *time, &mut out, &mut eye);
        }
    }
    for (e, t) in out {
        if let Ok(mut tf) = tfs.get_mut(e) {
            *tf = t;
        }
    }
    for (e, m) in eye {
        if let Ok(mut mm) = matq.get_mut(e) {
            if mm.0 != b.glow[m] {
                mm.0 = b.glow[m].clone();
            }
        }
    }
    let t_now = b.time;
    for i in 0..b.debris.len() {
        let (body, ent) = (b.debris[i].body, b.debris[i].ent);
        let Some(rb) = b.phys.w.bodies.get(body) else { continue };
        let (pos, rot) = (rb.translation(), *rb.rotation());
        if let Ok(mut tf) = tfs.get_mut(ent) {
            tf.translation = pos;
            tf.rotation = rot;
        }
        // freshly torn-off parts trail smoke for a couple of seconds
        let age = t_now - b.debris[i].born;
        if age < 2.5 && pdt > 0.0 {
            b.debris[i].smoke_t -= dt;
            if b.debris[i].smoke_t <= 0.0 {
                b.debris[i].smoke_t = 0.05 + age * 0.06;
                b.fx.smoke_puff(pos, 1, 0.18 + age * 0.08, 0.1 + 0.05 * (1.0 - age / 2.5));
                if age < 0.6 && crate::rng::chance(0.5) {
                    b.fx.fire(pos);
                }
            }
        }
    }
    let fdt = if pdt > 0.0 { dt * b.slowmo } else { dt * 0.15 };
    b.fx.update(fdt, &arena::floor_y);
    b.fx.write(&mut meshes);
    b.excitement = (b.excitement - dt * 0.35).max(0.0);
    let exc = b.excitement + if b.phase == Phase::Over { 1.0 } else { 0.0 };
    if let (Ok(mut c), Ok(mut l)) = (crowd.single_mut(), pit.single_mut()) {
        arena::update_arena(&mut meshes, &mut c, &mut l, time.elapsed_secs(), exc);
    }
    // camera
    if let Ok(mut c) = cam.single_mut() {
        let (pa, pb) = (b.bots[0].pos, b.bots[1].pos);
        let sep = pa.distance(pb);
        let mut target = pa * 0.7 + pb * 0.3;
        target.y = 0.0;
        target.x = target.x.clamp(-A + 1.0, A - 1.0);
        target.z = target.z.clamp(-A + 0.5, A - 2.0);
        if b.phase == Phase::Intro {
            target = Vec3::new(0.0, 0.0, -0.8);
        }
        let kk = 1.0 - (-dt * 3.0).exp();
        b.cam_target = b.cam_target.lerp(target, kk);
        let intro = if b.phase == Phase::Intro { 1.2 - (b.phase_t / 3.0).min(1.0) * 0.2 } else { 1.0 };
        let want = (5.5 + sep * 0.6).clamp(7.2, 15.0) * intro;
        b.cam_dist += (want - b.cam_dist) * kk;
        let el = 34f32.to_radians(); // low enough to see the bots' sides
        // aim a little past the action so the bots sit low in frame
        let tz = b.cam_target.z + 1.3;
        let pos = Vec3::new(b.cam_target.x, el.sin() * b.cam_dist, tz - el.cos() * b.cam_dist);
        *c = Transform::from_translation(pos).looking_at(Vec3::new(b.cam_target.x, 0.0, tz), Vec3::Y);
        c.translation += b.fx.shake_offset(time.elapsed_secs());
    }
    // test drive
    if b.test {
        if !b.bots[1].alive || b.bots[1].pos.y < -1.0 {
            b.respawn_t += dt;
            if b.respawn_t > 2.5 {
                b.respawn_t = 0.0;
                // the dummy is rebuilt in place
                let Battle { phys, part_of, bots, enemy_design, glow, .. } = b;
                bots[1].dispose(phys, part_of, &mut commands);
                let mut sp = Spawn { commands: &mut commands, meshes: &mut meshes, mats: &mut mats, shared: &shared, glow: glow.clone() };
                bots[1] = Robot::new(&mut sp, phys, part_of, enemy_design, Vec3::new(0.0, 0.0, 4.0), std::f32::consts::PI, 1, Scoped);
            }
        }
        if keys.just_pressed(KeyCode::KeyR) {
            let rb = b.phys.body_mut(b.bots[0].body);
            rb.set_translation(Vec3::new(0.0, 0.8, -5.0), true);
            rb.set_rotation(Quat::IDENTITY, true);
            rb.set_linvel(Vec3::ZERO, true);
            rb.set_angvel(Vec3::ZERO, true);
        }
    }
    if b.phase == Phase::Over && (b.phase_t > 3.2 || b.bench) && !b.test {
        if let Some(r) = b.result.take() {
            s.last = Some(r);
            next.set(Screen::Results);
        }
    }
}

// ------------------------------------------------------------------ HUD
fn build_hud(commands: &mut Commands, fonts: &Fonts, test: bool) {
    commands
        .spawn((Node { position_type: PositionType::Absolute, top: Val::Px(14.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, column_gap: Val::Px(16.0), ..default() }, Pickable::IGNORE, Scoped))
        .with_children(|top| {
            for (team, col) in [(0usize, Color::srgb_u8(0xff, 0x8a, 0x5c)), (1, Color::srgb_u8(0x7f, 0xb2, 0xff))] {
                if team == 1 {
                    top.spawn((ui::panel(Node { min_width: Val::Px(96.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }), Pickable::IGNORE)).with_children(|t| {
                        t.spawn((ui::title_text(fonts, "2:00", 50.0, ui::CREAM), TimerText, Pickable::IGNORE));
                    });
                }
                top.spawn((ui::panel(Node { width: Val::Px(300.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), ..default() }), Pickable::IGNORE)).with_children(|c| {
                    c.spawn((Node { justify_content: JustifyContent::SpaceBetween, ..default() }, Pickable::IGNORE)).with_children(|n| {
                        n.spawn((ui::title_text(fonts, "", 24.0, col), CoreText(team + 10), Pickable::IGNORE));
                        n.spawn((ui::text(fonts, "CORE 100%", 15.0, ui::CREAM), CoreText(team), Pickable::IGNORE));
                    });
                    c.spawn((Node { height: Val::Px(14.0), border_radius: BorderRadius::all(Val::Px(7.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)), Pickable::IGNORE)).with_children(|t| {
                        t.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(7.0)), ..default() }, BackgroundColor(ui::GREEN), HpFill(team), Pickable::IGNORE));
                    });
                    c.spawn((Node { column_gap: Val::Px(4.0), flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(3.0), ..default() }, Pips(team), Pickable::IGNORE));
                    c.spawn((Node { height: Val::Px(4.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)), Pickable::IGNORE)).with_children(|t| {
                        t.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, BackgroundColor(ui::TEAL), BoostFill(team), Pickable::IGNORE));
                    });
                });
            }
        });
    commands.spawn((
        Node { position_type: PositionType::Absolute, top: Val::Percent(58.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
        Pickable::IGNORE,
        Scoped,
    )).with_children(|c| {
        c.spawn((ui::title_text(fonts, "", 48.0, ui::YELLOW), TextShadow { offset: Vec2::new(0.0, 4.0), color: Color::srgba(0.0, 0.0, 0.0, 0.5) }, ImmobText, Pickable::IGNORE));
    });
    let hint = if test {
        "TEST DRIVE · WASD walk · CLICK weapon 1 · RIGHT-CLICK weapon 2 · SPACE handbrake · SHIFT boost · R reset · ESC back to workshop"
    } else {
        "WASD walk · CLICK / RIGHT-CLICK weapons · SPACE handbrake · SHIFT boost · hit their side or back for extra damage"
    };
    commands.spawn((Node { position_type: PositionType::Absolute, bottom: Val::Px(14.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE, Scoped)).with_children(|c| {
        c.spawn((ui::panel(Node { padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)), ..default() }), Pickable::IGNORE)).with_children(|p| {
            p.spawn((ui::text(fonts, hint, 16.0, ui::CREAM), Pickable::IGNORE));
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn hud(
    mut commands: Commands,
    mut b: ResMut<Battle>,
    fonts: Res<Fonts>,
    time: Res<Time>,
    mut texts: Query<(&mut Text, &mut TextColor, Option<&CoreText>, Option<&TimerText>, Option<&ImmobText>)>,
    mut fills: Query<(&mut Node, &mut BackgroundColor, Option<&HpFill>, Option<&BoostFill>), (Without<Pips>, Without<BannerNode>)>,
    pips: Query<(Entity, &Pips)>,
    mut banners: Query<(Entity, &mut BannerNode)>,
    cam: Query<(&Camera, &GlobalTransform), With<FightCam>>,
) {
    let dt = time.delta_secs();
    // banner: newest replaces the old one
    for (e, mut bn) in banners.iter_mut() {
        bn.0 -= dt;
        if bn.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
    if let Some((a, sub)) = b.banner.take() {
        for (e, _) in banners.iter() {
            commands.entity(e).despawn();
        }
        let life = if a.chars().count() < 4 { 0.75 } else { 2.6 };
        commands
            .spawn((
                Node { position_type: PositionType::Absolute, top: Val::Percent(26.0), width: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
                BannerNode(life),
                Pickable::IGNORE,
                GlobalZIndex(20),
                Scoped,
            ))
            .with_children(|c| {
                c.spawn((ui::title_text(&fonts, a, 104.0, ui::YELLOW), TextShadow { offset: Vec2::new(0.0, 7.0), color: Color::srgba(0.0, 0.0, 0.0, 0.5) }, Pickable::IGNORE));
                if !sub.is_empty() {
                    c.spawn((ui::title_text(&fonts, sub, 28.0, ui::CREAM), TextShadow { offset: Vec2::new(0.0, 3.0), color: Color::srgba(0.0, 0.0, 0.0, 0.5) }, Pickable::IGNORE));
                }
            });
    }
    if b.flash {
        b.flash = false;
        commands.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)), Flash(0.3), Pickable::IGNORE, GlobalZIndex(40), Scoped));
    }
    // pop words: spawned at the projected point
    if let Ok((camera, gt)) = cam.single() {
        for (at, big, word, color) in std::mem::take(&mut b.pops) {
            let Ok(px) = camera.world_to_viewport(gt, at) else { continue };
            let w = word.unwrap_or_else(|| if big { crate::rng::pick(&WORDS_BIG).into() } else { crate::rng::pick(&WORDS_SMALL).into() });
            commands.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(px.x - 80.0), top: Val::Px(px.y - 30.0), width: Val::Px(160.0), justify_content: JustifyContent::Center, ..default() },
                PopWord { t: 0.0, at: px },
                Pickable::IGNORE,
                GlobalZIndex(30),
                Scoped,
            )).with_children(|c| {
                c.spawn((ui::title_text(&fonts, w, if big { 46.0 } else { 24.0 }, color.unwrap_or(ui::YELLOW)), TextShadow { offset: Vec2::new(0.0, 4.0), color: Color::srgba(0.13, 0.12, 0.16, 0.9) }, TextLayout::default().with_no_wrap(), Pickable::IGNORE));
            });
        }
    }
    b.hud_t -= 1;
    if b.hud_t > 0 {
        return;
    }
    b.hud_t = 6;
    for (mut t, mut col, core, timer, immob) in texts.iter_mut() {
        if let Some(c) = core {
            if c.0 >= 10 {
                t.0 = b.bots[c.0 - 10].label.clone();
            } else {
                t.0 = format!("CORE {}%", (b.bots[c.0].hp_frac() * 100.0).ceil());
            }
        } else if timer.is_some() {
            if b.test {
                t.0 = "∞".into();
            } else {
                let tl = b.time_left.max(0.0).ceil() as i32;
                t.0 = format!("{}:{:02}", tl / 60, tl % 60);
                col.0 = if tl <= 10 { ui::RED } else { ui::CREAM };
            }
        } else if immob.is_some() {
            let mut msg = String::new();
            for k in 0..2 {
                if b.immob[k] > 3.0 && b.phase == Phase::Fight {
                    msg = format!("{} immobilised… {}", b.bots[k].label, (10.0 - b.immob[k]).ceil());
                }
            }
            t.0 = msg;
        }
    }
    for (mut n, mut bg, hp, boost) in fills.iter_mut() {
        if let Some(h) = hp {
            let r = &b.bots[h.0];
            let (mut tot, mut cur) = (0.0, 0.0);
            for p in &r.parts {
                tot += p.max_hp;
                if p.attached {
                    cur += p.hp;
                }
            }
            let f = cur / tot;
            n.width = Val::Percent(f * 100.0);
            bg.0 = if f < 0.3 { ui::RED } else if f < 0.6 { ui::YELLOW } else { ui::GREEN };
        }
        if let Some(bo) = boost {
            let r = &b.bots[bo.0];
            n.width = Val::Percent((1.0 - r.boost_cd / Boost::COOLDOWN) * 100.0);
            bg.0 = if r.boost_cd <= 0.0 { ui::TEAL } else { Color::srgb_u8(0x5a, 0x5f, 0x6a) };
        }
    }
    for (e, p) in pips.iter() {
        let r = &b.bots[p.0];
        commands.entity(e).despawn_related::<Children>().with_children(|c| {
            for part in &r.parts {
                let f = part.hp / part.max_hp;
                let col = if !part.attached { Color::srgba(1.0, 1.0, 1.0, 0.06) } else if f < 0.3 { ui::RED } else if f < 0.7 { ui::YELLOW } else { Color::srgb_u8(0x5a, 0x58, 0x66) };
                c.spawn((Node { width: Val::Px(18.0), height: Val::Px(9.0), border_radius: BorderRadius::all(Val::Px(3.0)), ..default() }, BackgroundColor(col), Pickable::IGNORE));
            }
        });
    }
}

fn popwords(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut PopWord, &mut Node, &Children)>, mut tc: Query<(&mut TextColor, &mut UiTransform)>) {
    for (e, mut p, mut n, ch) in q.iter_mut() {
        p.t += time.delta_secs();
        let k = p.t / 0.95;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        n.top = Val::Px(p.at.y - 30.0 - 46.0 * k.powf(0.6));
        for c in ch.iter() {
            if let Ok((mut col, mut t)) = tc.get_mut(c) {
                let pop = if k < 0.12 { 0.4 + k / 0.12 * 0.8 } else { 1.2 - (k - 0.12).min(0.2) };
                t.scale = Vec2::splat(pop);
                t.rotation = Rot2::radians(-0.08);
                col.0 = col.0.with_alpha(if k > 0.7 { 1.0 - (k - 0.7) / 0.3 } else { 1.0 });
            }
        }
    }
}

fn flash_fade(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Flash, &mut BackgroundColor)>) {
    for (e, mut f, mut bg) in q.iter_mut() {
        f.0 -= time.delta_secs();
        if f.0 <= 0.0 {
            commands.entity(e).despawn();
        } else {
            bg.0 = Color::srgba(1.0, 1.0, 1.0, f.0 / 0.3 * 0.8);
        }
    }
}
