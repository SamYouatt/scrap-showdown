//! Arena fight: the match simulation steps in `FixedUpdate`; `Update` systems
//! run the countdown, time dilation (slow motion and hit-stop through
//! `Time<Virtual>`), mirror the simulation onto entities, and drive the
//! camera. The HUD is in `hud`.
//!
//! The simulation itself (robots on a rapier world) is plain Rust owned by the
//! [`Battle`] resource. It doesn't touch the ECS directly for sounds, pop words
//! or banners: it queues them in an outbox that `flush_outbox` turns into
//! messages.
mod hud;

use super::Screen;
use crate::ai::{Brain, Hazard};
use crate::arena::{self, Crowd, PitLight, A, PIT};
use crate::audio::{Sfx, Snd};
use crate::fx::Fx;
use crate::models::{self, Mats};
use crate::params::param;
use crate::parts::{Category, Design, PartKind};
use crate::physics::{Phys, STEP};
use crate::rng;
use crate::art::Art;
use crate::robot::{Ctx, Debris, DetachCtx, Ev, EyeMats, Input, PartOf, Robot, Spawn};
use crate::scene::{self, SunOpts};
use crate::state::{self, BattleResult, Run};
use crate::theme::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;
use hud::Banner;
use rapier3d::prelude::ColliderHandle;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const MATCH_TIME: f32 = 120.0;
/// Most physics steps per frame; a slower frame drops time instead.
const MAX_STEPS_PER_FRAME: f32 = 4.0;

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(Time::<Fixed>::from_seconds(STEP as f64));
    app.add_systems(OnEnter(Screen::Fight), spawn_fight);
    app.add_systems(OnExit(Screen::Fight), end_fight);
    app.add_systems(FixedUpdate, step_fight.run_if(in_state(Screen::Fight)));
    app.add_systems(
        Update,
        (
            bench_steps.run_if(|b: Option<Res<Battle>>| b.is_some_and(|b| b.bench)),
            (tick_match, dilate_time, sync_robots, sync_debris, cheer_crowd, follow_camera).chain(),
            (respawn_dummy, reset_player.run_if(input_just_pressed(KeyCode::KeyR)), leave_test_drive.run_if(input_just_pressed(KeyCode::Escape)))
                .run_if(|b: Option<Res<Battle>>| b.is_some_and(|b| b.test)),
            show_results,
            flush_outbox,
        )
            .chain()
            .run_if(in_state(Screen::Fight)),
    );
    app.add_plugins(hud::plugin);
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

/// What the simulation wants shown or heard; flushed to messages each frame.
#[derive(Default)]
struct Outbox {
    sfx: Vec<Sfx>,
    pops: Vec<PopWord>,
    banner: Option<Banner>,
    flash: bool,
}

#[derive(Resource)]
pub struct Battle {
    phys: Phys,
    part_of: PartOf,
    pub bots: Vec<Robot>,
    brain: Brain,
    /// `?auto`: the AI drives the player too
    auto: Option<Brain>,
    debris: Vec<Debris>,
    /// test drive against the dummy from the workshop
    pub test: bool,
    enemy_design: Design,
    /// hit-stop seconds left (real time)
    freeze: f32,
    slowmo: f32,
    slowmo_t: f32,
    /// simulation clock
    time: f32,
    pub phase: Phase,
    /// real seconds in the current phase
    phase_t: f32,
    pub time_left: f32,
    pub immob: [f32; 2],
    // last place each bot was seen really moving, and how long since
    still: [Option<(Vec3, f32, f32)>; 2],
    pair_cd: HashMap<PairKey, f32>,
    pre_vel: [Vec3; 2],
    excitement: f32,
    result: Option<BattleResult>,
    countdown: i32,
    player_lost: Vec<usize>,
    respawn_t: f32,
    walls: HashSet<ColliderHandle>,
    ev: Vec<Ev>,
    out: Outbox,
    /// headless-ish benchmark: run the sim as fast as possible and log the result
    bench: bool,
}

#[derive(Component)]
struct FightCam {
    target: Vec3,
    dist: f32,
}

const WORDS_BIG: [&str; 6] = ["CRUNCH!", "KA-BLAM!", "WHAM!", "CLANG!", "SMASH!", "KERRANG!"];
const WORDS_SMALL: [&str; 4] = ["bonk", "tink", "clonk", "dink"];

// ------------------------------------------------------------------ enter / exit
#[allow(clippy::too_many_arguments)]
fn spawn_fight(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shared: Res<Mats>,
    eyes: Res<EyeMats>,
    art: Res<Art>,
    s: Res<Run>,
    fonts: Res<Fonts>,
    mut virt: ResMut<Time<Virtual>>,
) {
    let scope = DespawnOnExit(Screen::Fight);
    let opps = state::opponents();
    let test = param("test").is_some() || s.testing;
    let oi = param("opp").and_then(|v| v.parse::<usize>().ok()).unwrap_or(s.round).min(opps.len() - 1);
    let (enemy, skill) = if test { (state::dummy(), 0.0) } else { (opps[oi].design.clone(), opps[oi].skill) };
    let mut phys = Phys::new();
    scene::sun_lights(&mut commands, SunOpts { size: 14.0, dir: Vec3::new(-10.0, 9.0, -7.0), ..default() }, scope.clone());
    models::reseed(7);
    scene::sky_and_clouds(&mut commands, &mut meshes, &mut mats, Color::srgb_u8(0x4f, 0x9f, 0xe0), Color::srgb_u8(0xcf, 0xe9, 0xf7), 10, 60.0, scope.clone());
    let info = arena::build_arena(&mut commands, &mut meshes, &mut mats, &shared, &art, &mut phys, scope.clone());
    let fx = Fx::new(&mut commands, &mut meshes, &mut mats, arena::floor_y, scope.clone());
    commands.insert_resource(fx);
    let mut out = Outbox::default();
    let mut part_of = PartOf::new();
    let mut sp = Spawn { commands: &mut commands, meshes: &mut meshes, glow: &eyes, art: &art, sfx: &mut out.sfx };
    let player = Robot::new(&mut sp, &mut phys, &mut part_of, &s.design, Vec3::new(0.0, 0.0, -6.0), 0.0, 0, scope.clone());
    let enemy_bot = Robot::new(&mut sp, &mut phys, &mut part_of, &enemy, Vec3::new(0.0, 0.0, 6.0), std::f32::consts::PI, 1, scope.clone());
    let hz = vec![Hazard { x: PIT.x, z: PIT.z, r: PIT.h }];
    let bench = param("bench").is_some();
    commands.spawn((
        scene::camera_bundle(40.0, Transform::from_xyz(0.0, 14.0, -14.0).looking_at(Vec3::ZERO, Vec3::Y)),
        // haze pushes the stands back
        DistanceFog { color: Color::srgb_u8(0xc9, 0xd9, 0xef), falloff: FogFalloff::Linear { start: 18.0, end: 70.0 }, ..default() },
        FightCam { target: Vec3::ZERO, dist: 14.0 },
        scope.clone(),
    ));
    out.sfx.push(Sfx::Crowd(true));
    let battle = Battle {
        phys,
        part_of,
        bots: vec![player, enemy_bot],
        brain: Brain::new(skill, hz.clone()),
        auto: param("auto").map(|_| Brain::new(0.7, hz)),
        debris: vec![],
        test,
        enemy_design: enemy,
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
        result: None,
        countdown: -1,
        player_lost: vec![],
        respawn_t: 0.0,
        walls: info.walls,
        ev: vec![],
        out,
        bench,
    };
    hud::spawn_hud(&mut commands, &fonts, &battle, scope);
    commands.insert_resource(battle);
    virt.set_max_delta(Duration::from_secs_f32(STEP * MAX_STEPS_PER_FRAME));
}

fn end_fight(mut commands: Commands, b: Res<Battle>, mut sfx: MessageWriter<Sfx>, mut virt: ResMut<Time<Virtual>>) {
    sfx.write(Sfx::Crowd(false));
    let mut stops = vec![];
    for r in &b.bots {
        r.stop_hums(&mut stops);
    }
    sfx.write_batch(stops);
    virt.unpause();
    virt.set_relative_speed(1.0);
    virt.set_max_delta(Duration::from_millis(250));
    commands.remove_resource::<Battle>();
    commands.remove_resource::<Fx>();
}

// ------------------------------------------------------------------ simulation
#[derive(Default)]
struct PlayerKeys {
    fwd: f32,
    turn: f32,
    fire: [bool; 2],
    brake: bool,
    boost: bool,
}

impl PlayerKeys {
    fn read(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>, free_mouse: bool) -> Self {
        let k = |c: &[KeyCode]| c.iter().any(|x| keys.pressed(*x));
        PlayerKeys {
            fwd: (k(&[KeyCode::KeyW, KeyCode::ArrowUp]) as i32 - k(&[KeyCode::KeyS, KeyCode::ArrowDown]) as i32) as f32,
            turn: (k(&[KeyCode::KeyD, KeyCode::ArrowRight]) as i32 - k(&[KeyCode::KeyA, KeyCode::ArrowLeft]) as i32) as f32,
            fire: [
                (free_mouse && mouse.pressed(MouseButton::Left)) || k(&[KeyCode::KeyF, KeyCode::KeyJ]),
                (free_mouse && mouse.pressed(MouseButton::Right)) || k(&[KeyCode::KeyG, KeyCode::KeyL]),
            ],
            brake: k(&[KeyCode::Space]),
            boost: k(&[KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::KeyK]),
        }
    }
}

fn step_fight(
    mut commands: Commands,
    mut b: ResMut<Battle>,
    mut fx: ResMut<Fx>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    over_ui: Res<PointerOverUi>,
) {
    let pk = PlayerKeys::read(&keys, &mouse, !over_ui.0);
    b.fixed_step(&mut commands, &mut fx, &pk);
}

/// `?bench`: on top of the normal fixed steps, run the simulation flat out
/// for ~250 ms a frame.
fn bench_steps(world: &mut World) {
    let t0 = bevy::platform::time::Instant::now();
    while t0.elapsed().as_secs_f32() < 0.25 && world.resource::<Battle>().phase != Phase::Over {
        world.run_schedule(FixedUpdate);
    }
}

// ------------------------------------------------------------------ match flow
/// Phase clock and the 3-2-1 countdown.
fn tick_match(real: Res<Time<Real>>, mut b: ResMut<Battle>) {
    b.phase_t += real.delta_secs().min(0.05);
    if b.phase != Phase::Intro {
        return;
    }
    let steps = ["3", "2", "1", "ACTIVATE!"];
    let st = (b.phase_t / 0.8).floor() as i32 - 1;
    if st != b.countdown && (0..4).contains(&st) {
        b.countdown = st;
        b.set_banner(steps[st as usize], if st == 3 { "WASD walk · CLICK fire · SPACE handbrake · SHIFT boost" } else { "" });
        if st == 3 {
            b.out.sfx.extend([Snd::Buzzer.play(), Snd::Cheer.at(1.0)]);
            b.phase = Phase::Fight;
            b.phase_t = 0.0;
        } else {
            b.out.sfx.push(Snd::Bell.play());
        }
    }
}

/// Slow motion after the bell and hit-stop on big hits, by scaling and
/// pausing virtual time (which drives `FixedUpdate`).
fn dilate_time(real: Res<Time<Real>>, mut virt: ResMut<Time<Virtual>>, mut b: ResMut<Battle>, mut fx: ResMut<Fx>) {
    let dt = real.delta_secs().min(0.05);
    if b.slowmo_t > 0.0 {
        b.slowmo_t -= dt;
        if b.slowmo_t <= 0.0 {
            b.slowmo = 1.0;
        }
    }
    if b.bench {
        b.freeze = 0.0;
    }
    if b.freeze > 0.0 {
        b.freeze -= dt;
        virt.pause();
        fx.time_scale = 0.15;
    } else {
        virt.unpause();
        virt.set_relative_speed(b.slowmo);
        fx.time_scale = b.slowmo;
    }
}

fn show_results(mut b: ResMut<Battle>, mut s: ResMut<Run>, mut next: ResMut<NextState<Screen>>) {
    if b.phase == Phase::Over && (b.phase_t > 3.2 || b.bench) && !b.test
        && let Some(r) = b.result.take() {
            s.last = Some(r);
            next.set(Screen::Results);
        }
}

fn flush_outbox(mut b: ResMut<Battle>, mut sfx: MessageWriter<Sfx>, mut pops: MessageWriter<PopWord>, mut banners: MessageWriter<Banner>, mut flash: MessageWriter<Flash>) {
    let out = std::mem::take(&mut b.out);
    sfx.write_batch(out.sfx);
    pops.write_batch(out.pops);
    if let Some(banner) = out.banner {
        banners.write(banner);
    }
    if out.flash {
        flash.write(Flash);
    }
}

// ------------------------------------------------------------------ test drive
/// The dummy is rebuilt in place a moment after it dies or falls out.
fn respawn_dummy(mut commands: Commands, real: Res<Time<Real>>, mut b: ResMut<Battle>, mut meshes: ResMut<Assets<Mesh>>, eyes: Res<EyeMats>, art: Res<Art>) {
    if b.bots[1].alive && b.bots[1].pos.y >= -1.0 {
        return;
    }
    b.respawn_t += real.delta_secs().min(0.05);
    if b.respawn_t <= 2.5 {
        return;
    }
    b.respawn_t = 0.0;
    let Battle { phys, part_of, bots, enemy_design, out, .. } = &mut *b;
    bots[1].dispose(phys, part_of, &mut commands, &mut out.sfx);
    let mut sp = Spawn { commands: &mut commands, meshes: &mut meshes, glow: &eyes, art: &art, sfx: &mut out.sfx };
    bots[1] = Robot::new(&mut sp, phys, part_of, enemy_design, Vec3::new(0.0, 0.0, 4.0), std::f32::consts::PI, 1, DespawnOnExit(Screen::Fight));
}

fn reset_player(mut b: ResMut<Battle>) {
    let body = b.bots[0].body;
    let rb = b.phys.body_mut(body);
    rb.set_translation(Vec3::new(0.0, 0.8, -5.0), true);
    rb.set_rotation(Quat::IDENTITY, true);
    rb.set_linvel(Vec3::ZERO, true);
    rb.set_angvel(Vec3::ZERO, true);
}

fn leave_test_drive(mut s: ResMut<Run>, mut next: ResMut<NextState<Screen>>) {
    s.testing = false;
    next.set(Screen::Workshop);
}

// ------------------------------------------------------------------ presentation
/// Copy robot poses, wheel spin, legs and eye moods onto their entities.
fn sync_robots(
    real: Res<Time<Real>>,
    virt: Res<Time<Virtual>>,
    mut b: ResMut<Battle>,
    mut fx: ResMut<Fx>,
    eyes: Res<EyeMats>,
    mut tfs: Query<&mut Transform>,
    mut matq: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let vdt = if virt.is_paused() { 0.0 } else { real.delta_secs().min(0.05) };
    let mut out = vec![];
    let mut eye = vec![];
    let Battle { bots, phys, out: outbox, .. } = &mut *b;
    for r in bots.iter_mut() {
        r.sync_visual(phys, &mut fx, &mut outbox.sfx, vdt, &mut out, &mut eye);
    }
    for (e, t) in out {
        if let Ok(mut tf) = tfs.get_mut(e) {
            *tf = t;
        }
    }
    for (e, m) in eye {
        if let Ok(mut mm) = matq.get_mut(e) {
            let want = eyes.get(m);
            if mm.0 != want {
                mm.0 = want;
            }
        }
    }
}

/// Torn-off parts follow their physics bodies and trail smoke for a bit.
fn sync_debris(real: Res<Time<Real>>, virt: Res<Time<Virtual>>, mut b: ResMut<Battle>, mut fx: ResMut<Fx>, mut tfs: Query<&mut Transform>) {
    let dt = real.delta_secs().min(0.05);
    let b = &mut *b;
    for d in b.debris.iter_mut() {
        let Some(rb) = b.phys.w.bodies.get(d.body) else { continue };
        let (pos, rot) = (rb.translation(), *rb.rotation());
        if let Ok(mut tf) = tfs.get_mut(d.ent) {
            tf.translation = pos;
            tf.rotation = rot;
        }
        let age = b.time - d.born;
        if age < 2.5 && !virt.is_paused() {
            d.smoke_t -= dt;
            if d.smoke_t <= 0.0 {
                d.smoke_t = 0.05 + age * 0.06;
                fx.smoke_puff(pos, 1, 0.18 + age * 0.08, 0.1 + 0.05 * (1.0 - age / 2.5));
                if age < 0.6 && rng::chance(0.5) {
                    fx.fire(pos);
                }
            }
        }
    }
}

/// The crowd bobs and the pit light pulses with the excitement.
fn cheer_crowd(real: Res<Time<Real>>, mut b: ResMut<Battle>, mut tfs: Query<&mut Transform>, crowd: Single<&Crowd>, mut pit: Single<&mut PointLight, With<PitLight>>) {
    b.excitement = (b.excitement - real.delta_secs().min(0.05) * 0.35).max(0.0);
    let exc = b.excitement + if b.phase == Phase::Over { 1.0 } else { 0.0 };
    arena::update_arena(&mut tfs, &crowd, &mut pit, real.elapsed_secs(), exc);
}

/// Frame both bots (favouring yours), pulling in as they close.
fn follow_camera(real: Res<Time<Real>>, b: Res<Battle>, fx: Res<Fx>, cam: Single<(&mut Transform, &mut FightCam)>) {
    let (mut tf, mut c) = cam.into_inner();
    let dt = real.delta_secs().min(0.05);
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
    c.target = c.target.lerp(target, kk);
    let intro = if b.phase == Phase::Intro { 1.2 - (b.phase_t / 3.0).min(1.0) * 0.2 } else { 1.0 };
    let want = (5.5 + sep * 0.6).clamp(7.2, 15.0) * intro;
    c.dist += (want - c.dist) * kk;
    let el = 34f32.to_radians(); // low enough to see the bots' sides
    // aim a little past the action so the bots sit low in frame
    let tz = c.target.z + 1.3;
    let pos = Vec3::new(c.target.x, el.sin() * c.dist, tz - el.cos() * c.dist);
    *tf = Transform::from_translation(pos).looking_at(Vec3::new(c.target.x, 0.0, tz), Vec3::Y);
    tf.translation += fx.shake_offset(real.elapsed_secs());
}

impl Battle {
    fn hit_stop(&mut self, s: f32) {
        self.freeze = self.freeze.max(s);
    }
    /// A pop word over the action; without a `word`, a random sound effect.
    fn pop(&mut self, at: Vec3, big: bool, word: Option<&str>, color: Option<Color>) {
        let word = word.map(str::to_string).unwrap_or_else(|| (if big { rng::pick(&WORDS_BIG) } else { rng::pick(&WORDS_SMALL) }).to_string());
        self.out.pops.push(PopWord { at, big, word, color: color.unwrap_or(YELLOW) });
    }
    fn set_banner(&mut self, title: &str, sub: &str) {
        self.out.banner = Some(Banner { title: title.into(), sub: sub.into() });
    }

    fn part_world_pos(&self, (t, i): (usize, usize)) -> Vec3 {
        let r = &self.bots[t];
        r.part_world(&r.parts[i], Vec3::ZERO)
    }

    /// Apply damage to a part (may knock it off or kill the bot).
    fn damage(&mut self, commands: &mut Commands, fx: &mut Fx, (t, i): (usize, usize), dmg: f32, dir: Option<Vec3>) {
        let mut detached = vec![];
        let Battle { phys, part_of, debris, bots, time, out, .. } = self;
        let mut dc = DetachCtx { phys, part_of, fx, sfx: &mut out.sfx, commands, debris, detached: &mut detached, hit_stop: 0.0, time: *time };
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

    fn on_hit(&mut self, commands: &mut Commands, fx: &mut Fx, att: Option<usize>, tgt: (usize, usize), mut dmg: f32, pt: Vec3, big: bool, word: Option<&'static str>) {
        if self.phase == Phase::Over && !self.test {
            dmg *= 0.3;
        }
        let dir = att.map(|a| (self.bots[tgt.0].pos - self.bots[a].pos).with_y(0.3).normalize_or_zero());
        self.damage(commands, fx, tgt, dmg, dir);
        // the juice: chunks of the victim's paint and plate fly off, more for bigger hits
        if dmg > 3.0 {
            let n = ((2.0 + dmg * 0.45) as usize).min(26);
            let paint = self.bots[tgt.0].paint;
            fx.shard_burst(pt, n, paint, dir, 3.0 + (dmg / 8.0).min(5.0));
            if dmg > 15.0 {
                fx.bolt_burst(pt, (dmg / 8.0) as usize, dir);
            }
        }
        if let Some(a) = att
            && a != tgt.0 {
                self.bots[a].damage_dealt += dmg;
            }
        let i = (dmg / 40.0).min(1.0);
        self.excitement = (self.excitement + i * 0.8).min(1.5);
        self.out.sfx.push(Snd::Cheer.at(i));
        let green = Some(Color::srgb_u8(0x7c, 0xcf, 0x5a));
        let at = pt + Vec3::Y * 0.6;
        if big {
            self.pop(at, true, word, if word.is_some() { green } else { None });
        } else if word.is_some() {
            self.pop(at, false, word, green);
        } else if dmg > 6.0 && rng::chance(0.4) {
            self.pop(pt + Vec3::Y * 0.5, false, None, None);
        }
    }

    fn on_detach(&mut self, (t, i): (usize, usize)) {
        self.excitement = (self.excitement + 0.4).min(1.5);
        self.out.sfx.push(Snd::Cheer.at(0.6));
        // parts blown off by the final KO explosion or after the bell aren't "lost"
        if t == 0 && self.bots[0].alive && self.phase != Phase::Over {
            self.player_lost.push(i);
        }
        if rng::chance(0.6) {
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

    fn fixed_step(&mut self, commands: &mut Commands, fx: &mut Fx, keys: &PlayerKeys) {
        self.time += STEP;
        // input
        let fighting = self.phase == Phase::Fight;
        self.bots[0].input = if let Some(auto) = self.auto.as_mut().filter(|_| fighting) {
            let (a, b) = self.bots.split_at(1);
            auto.think(&a[0], &b[0], &self.phys, STEP, self.time)
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
            let Battle { phys, part_of, ev, bots, time, out, .. } = self;
            let mut ctx = Ctx { phys, fx, sfx: &mut out.sfx, part_of, ev, time: *time };
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
                Ev::Hit { att, tgt, dmg, pt, big, word } => self.on_hit(commands, fx, att, tgt, dmg, pt, big, word),
            }
        }
        for f in forces {
            self.on_force(commands, fx, f.collider1, f.collider2, f.total_force_magnitude);
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

    fn on_force(&mut self, commands: &mut Commands, fx: &mut Fx, h1: ColliderHandle, h2: ColliderHandle, f: f32) {
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
                self.on_hit(commands, fx, Some(p1.0), p2, d12, mid, i > 0.55 && d12 >= d21, if f12 && d12 > 10.0 { Some("FLANKED!") } else { None });
                self.on_hit(commands, fx, Some(p2.0), p1, d21, mid, i > 0.55 && d21 > d12, if f21 && d21 > 10.0 { Some("FLANKED!") } else { None });
                let n = (b - a).normalize_or_zero();
                fx.spark_burst(mid, (8.0 + 30.0 * i) as usize, Some(n.cross(Vec3::Y)), 6.0, 1.0);
                fx.bolt_burst(mid, (i * 6.0) as usize, None);
                self.out.sfx.push(Snd::Thud.at(0.3 + i * 0.7));
                fx.add_shake(0.15 + i * 0.45);
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
                    self.damage(commands, fx, p, 6.0 * i * resist(k), None);
                    fx.spark_burst(pos, (6.0 + 20.0 * i) as usize, None, 5.0, 1.0);
                    fx.dust(pos.with_y(0.1), 4);
                    self.out.sfx.push(Snd::Thud.at(0.25 + i * 0.6));
                    fx.add_shake(0.1 + i * 0.3);
                    if i > 0.4 {
                        self.pop(pos.with_y(1.0), false, Some("BONK"), None);
                    }
                } else if f > 5000.0 {
                    // floor slam after being thrown
                    self.pair_cd.insert(key, 0.4);
                    let i = ((f - 5000.0) / 12000.0).min(1.0);
                    let pos = self.part_world_pos(p);
                    if i > 0.15 {
                        self.damage(commands, fx, p, 5.0 * i, None);
                        fx.dust(pos.with_y(0.05), (4.0 + i * 10.0) as usize);
                        self.out.sfx.push(Snd::Thud.at(0.3 + i * 0.6));
                        fx.add_shake(0.1 + i * 0.4);
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
        self.out.flash = true;
        self.out.sfx.extend([Snd::Buzzer.play(), Snd::Cheer.at(1.5)]);
        self.excitement = 1.5;
        let sub = format!("{} wins!", self.bots[winner].label);
        self.set_banner(how, &sub);
        // trophy: one of the enemy's surviving (or fallen) parts
        let mut trophy = None;
        if won {
            let pool: Vec<PartKind> = self.bots[1].parts.iter().map(|p| p.kind).filter(|k| !matches!(k, PartKind::Core | PartKind::Block)).collect();
            let weapons: Vec<PartKind> = pool.iter().copied().filter(|k| k.def().category == Category::Weapon).collect();
            let pick = if !weapons.is_empty() && rng::chance(0.7) { &weapons } else { &pool };
            if !pick.is_empty() {
                trophy = Some(rng::pick(pick));
            }
        }
        let p = &self.bots[0];
        self.result = Some(BattleResult {
            won,
            how: how.into(),
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

