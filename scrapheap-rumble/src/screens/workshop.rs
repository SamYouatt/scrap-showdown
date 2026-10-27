//! The workshop: KSP-style freeform building on a turntable. Parts snap onto
//! any face of a structural part; roll/tilt/mirror before you bolt them on.
//!
//! State is split by how often it changes: [`Orbit`] (camera, every frame),
//! [`Hover`] (what the cursor points at, every frame) and [`Tool`] (the
//! current selection and undo stack). The side panels rebuild only when
//! `Tool` or the [`Run`] change.
use super::Screen;
use crate::audio::{Sfx, Snd};
use crate::models;
use crate::parts::{compute_stats, face_point, mirror_xf, orient_on, Category, Design, Face, PartKind, Placed, Res as Stock, KIND_ORDER};
use crate::art::Art;
use crate::partmodels::PartLook;
use crate::preview::{part_look, spawn_preview, spawn_static_part};
use crate::robot::EyeMats;
use crate::scene::{self, SunOpts};
use crate::state::{self, InvItem, Run, FAB_COST};
use crate::theme::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::f32::consts::{FRAC_PI_4, TAU};

const BENCH_Y: f32 = 0.9;
const SNAP: f32 = 0.05;
const TILTS: [f32; 4] = [0.0, 0.4, 0.8, -0.4];
const PAINTS: [&str; 8] = ["#e2483d", "#f2b632", "#3f7fd8", "#3fa7a3", "#7c5bb5", "#5a8a3a", "#ee7a32", "#e8e0d0"];
const BG: Color = Color::srgb(0.18, 0.149, 0.125); // #2e2620, a dark warm roof
/// how far the pointer may move before a press becomes an orbit drag
const DRAG_PX: f32 = 6.0;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Workshop), spawn_workshop);
    app.add_systems(OnExit(Screen::Workshop), |mut commands: Commands| {
        commands.remove_resource::<Orbit>();
        commands.remove_resource::<Tool>();
        commands.remove_resource::<Hover>();
    });
    app.add_systems(
        Update,
        (
            (orbit_camera, shortcut_keys, pick, handle_clicks).chain(),
            rebuild_bot.run_if(resource_changed::<Run>),
            (refresh_left, refresh_right, refresh_tools, update_fight_label).run_if(resource_exists_and_changed::<Tool>.or_eager(resource_changed::<Run>)),
            (show_ghosts, hover_outline, cursor_tip),
        )
            .chain()
            .run_if(in_state(Screen::Workshop)),
    );
    app.add_observer(on_action);
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Sel {
    Kind(PartKind),
    Remove,
}

#[derive(Clone, Copy)]
struct Xf {
    pos: Vec3,
    rot: Quat,
}

/// Where a part (and its mirrored twin) would go if you clicked now.
struct Plan {
    kind: PartKind,
    /// why it can't go there
    why: Option<String>,
    a: Xf,
    a_parent: u32,
    b: Option<(Xf, u32)>,
}

/// A press on the bench: becomes an orbit drag once it moves far enough.
struct Press {
    button: MouseButton,
    moved: f32,
    last: Vec2,
}

#[derive(Resource)]
struct Orbit {
    az: f32,
    elev: f32,
    dist: f32,
    press: Option<Press>,
}

impl Orbit {
    fn dragging(&self) -> bool {
        self.press.as_ref().is_some_and(|p| p.moved > DRAG_PX)
    }
}

/// The part under the cursor, and the placement plan for the selected kind.
#[derive(Resource, Default)]
struct Hover {
    part: Option<u32>,
    plan: Option<Plan>,
}

type Snapshot = (Design, Vec<InvItem>, i32, i32);

#[derive(Resource)]
struct Tool {
    sel: Option<Sel>,
    /// which bin item of the selected kind to bolt on next (None = healthiest)
    sel_uid: Option<u32>,
    roll: f32,
    tilt: f32,
    mirror: bool,
    undo: Vec<Snapshot>,
}

/// A workshop button's action, handled by [`on_action`].
#[derive(Component, Clone, Copy, Debug)]
enum WsAction {
    TestDrive,
    Fight,
    Mirror,
    Roll,
    Tilt,
    Undo,
    Unbolt,
    Kind(PartKind),
    Item(u32),
    FixSelected,
    RepairAll,
    Fabricate(usize),
    Cheat,
    Opponent(usize),
    Paint(usize),
}

#[derive(Component)]
struct WsCam;
#[derive(Component)]
struct WsBot;
#[derive(Component)]
struct Ghost(usize, Handle<StandardMaterial>);
#[derive(Component)]
struct LeftCol;
#[derive(Component)]
struct RightCol;
#[derive(Component)]
struct ToolsRow;
#[derive(Component)]
struct FightLabel;
#[derive(Component)]
struct Tip;

/// Toasts and sounds, the workshop's feedback channels.
#[derive(SystemParam)]
struct Feedback<'w> {
    toasts: MessageWriter<'w, Toast>,
    sfx: MessageWriter<'w, Sfx>,
}

impl Feedback<'_> {
    fn toast(&mut self, msg: impl Into<String>, col: Color) {
        self.toasts.write(Toast(msg.into(), col));
    }
    fn play(&mut self, s: Sfx) {
        self.sfx.write(s);
    }
}

/// Height of the bot's core above the floor when it stands on the bench.
fn bot_y(d: &Design) -> f32 {
    BENCH_Y + 0.06 - crate::parts::ground_level(d)
}

// ------------------------------------------------------------------ helpers
/// Oriented-box overlap (separating axis test) between two placed parts'
/// bounds, shrunk a little so neighbours may touch.
fn overlaps(ka: PartKind, a: Xf, kb: PartKind, b: Xf) -> bool {
    let obb = |k: PartKind, x: Xf| {
        let bd = k.def().bounds;
        let ax = [x.rot * Vec3::X, x.rot * Vec3::Y, x.rot * Vec3::Z];
        (x.rot * bd.c + x.pos, ax, (bd.h - Vec3::splat(0.06)).max(Vec3::splat(0.005)))
    };
    let (ca, aa, ha) = obb(ka, a);
    let (cb, ab, hb) = obb(kb, b);
    let t = cb - ca;
    let mut axes: Vec<Vec3> = aa.iter().chain(ab.iter()).copied().collect();
    for u in aa {
        for w in ab {
            let c = u.cross(w);
            if c.length_squared() > 1e-6 {
                axes.push(c.normalize());
            }
        }
    }
    for l in axes {
        let ra: f32 = (0..3).map(|i| (ha[i] * aa[i].dot(l)).abs()).sum();
        let rb: f32 = (0..3).map(|i| (hb[i] * ab[i].dot(l)).abs()).sum();
        if t.dot(l).abs() > ra + rb {
            return false;
        }
    }
    true
}

fn xf(p: &Placed) -> Xf {
    Xf { pos: p.pos, rot: p.rot }
}

/// Spend plate/bolts on one part. Some(true) = fixed, Some(false) = patched, None = broke.
fn fix(plate: &mut i32, bolts: &mut i32, kind: PartKind, dmg: &mut f32) -> Option<bool> {
    let (res, n) = state::repair_cost(kind, *dmg);
    let have = if res == Stock::Plate { &mut *plate } else { &mut *bolts };
    if *have >= n {
        *have -= n;
        *dmg = 0.0;
        return Some(true);
    }
    if *have <= 0 {
        return None;
    }
    *dmg *= 1.0 - *have as f32 / n as f32;
    *have = 0;
    Some(false)
}

fn validate(s: &Run, fb: &mut Feedback) -> bool {
    if compute_stats(&s.design).drive == 0.0 {
        fb.toast("Your bot needs legs or wheels that touch the floor!", RED);
        return false;
    }
    true
}

impl Tool {
    fn snapshot(&mut self, s: &Run) {
        self.undo.push((s.design.clone(), s.inv.clone(), s.plate, s.bolts));
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
    }

    fn kind(&self) -> Option<PartKind> {
        match self.sel {
            Some(Sel::Kind(k)) => Some(k),
            _ => None,
        }
    }

    /// The bin item the next placement uses: the chosen one, else the healthiest.
    fn pick<'a>(&self, s: &'a Run, kind: PartKind) -> Option<&'a InvItem> {
        let mut items: Vec<&InvItem> = s.inv.iter().filter(|x| x.kind == kind).collect();
        if let Some(it) = items.iter().find(|x| Some(x.uid) == self.sel_uid) {
            return Some(it);
        }
        items.sort_by(|a, b| a.dmg.total_cmp(&b.dmg));
        items.first().copied()
    }

    fn take(&self, s: &mut Run, kind: PartKind) -> f32 {
        let Some(uid) = self.pick(s, kind).map(|x| x.uid) else { return 0.0 };
        let i = s.inv.iter().position(|x| x.uid == uid).unwrap();
        s.inv.remove(i).dmg
    }

    fn make_plan(&self, s: &Run, kind: PartKind, par: &Placed, face: Face, u: f32, v: f32) -> Plan {
        let d = &s.design;
        let (point, normal) = face_point(par, face, u, v);
        let a = Xf { pos: point, rot: orient_on(normal, self.roll, self.tilt) };
        let mut b = None;
        if self.mirror && a.pos.x.abs() > 0.03 {
            let tp = par.mirror.or(if par.pos.x.abs() < 0.03 { Some(par.id) } else { None });
            if let Some(tp) = tp {
                let (mp, mr) = mirror_xf(a.pos, a.rot);
                b = Some((Xf { pos: mp, rot: mr }, tp));
            }
        }
        let have = s.count_of(kind);
        // only one left, or the twin would sit on top of it: place a single
        if b.is_some_and(|(bx, _)| have < 2 || overlaps(kind, a, kind, bx)) {
            b = None;
        }
        let need = if b.is_some() { 2 } else { 1 };
        let mut why = None;
        if !par.kind.def().structural {
            why = Some(format!("Can't bolt onto a {}", par.kind.def().name));
        } else if have < need {
            why = Some(format!("No {}s left", kind.def().name));
        } else {
            for (cand, pid) in [(Some(a), par.id), (b.map(|x| x.0), b.map_or(0, |x| x.1))] {
                let Some(cand) = cand else { continue };
                if let Some(hit) = d.parts.iter().find(|x| x.id != pid && overlaps(kind, cand, x.kind, xf(x))) {
                    why = Some(format!("Hits the {}", hit.kind.def().name));
                    break;
                }
            }
        }
        Plan { kind, why, a, a_parent: par.id, b }
    }

    fn place(&mut self, s: &mut Run, p: &Plan, fb: &mut Feedback) {
        if let Some(w) = &p.why {
            fb.toast(w.clone(), RED);
            fb.play(Snd::Click.play());
            return;
        }
        self.snapshot(s);
        let id = s.design.next_id;
        s.design.next_id += 1;
        let dmg = self.take(s, p.kind);
        let mut a = Placed { id, kind: p.kind, parent: Some(p.a_parent), pos: p.a.pos, rot: p.a.rot, dmg, mirror: None };
        if let Some((bx, bp)) = p.b {
            // the twin uses the healthiest remaining one
            let tid = s.design.next_id;
            s.design.next_id += 1;
            self.sel_uid = None;
            let bd = self.take(s, p.kind);
            s.design.parts.push(Placed { id: tid, kind: p.kind, parent: Some(bp), pos: bx.pos, rot: bx.rot, dmg: bd, mirror: Some(id) });
            a.mirror = Some(tid);
        }
        s.design.parts.push(a);
        fb.play(Snd::Place.play());
        self.sel_uid = None;
        if s.count_of(p.kind) == 0 {
            self.sel = None;
        }
    }

    fn remove(&mut self, s: &mut Run, id: u32, pick_up: bool, fb: &mut Feedback) {
        let Some(p) = s.design.by_id(id).cloned() else { return };
        if p.kind == PartKind::Core {
            fb.toast("The Brain Box stays put.", RED);
            return;
        }
        let mut ids = s.design.subtree(id);
        let twin = p.mirror.and_then(|m| s.design.by_id(m).map(|t| t.id));
        if let (true, Some(t)) = (self.mirror, twin) {
            for x in s.design.subtree(t) {
                if !ids.contains(&x) {
                    ids.push(x);
                }
            }
        }
        self.snapshot(s);
        let gone: Vec<Placed> = s.design.parts.iter().filter(|x| ids.contains(&x.id)).cloned().collect();
        s.design.parts.retain(|x| !ids.contains(&x.id));
        for x in s.design.parts.iter_mut() {
            if x.mirror.is_some_and(|m| ids.contains(&m)) {
                x.mirror = None;
            }
        }
        // each part goes back in the bin as-is, wear and all
        let mut held = None;
        for x in &gone {
            let uid = s.add_item(x.kind, x.dmg);
            if x.id == id {
                held = Some(uid);
            }
        }
        let extra = gone.len() as i32 - if twin.is_some() && self.mirror { 2 } else { 1 };
        if extra > 0 {
            fb.toast(format!("{extra} part{} attached to it came off too", if extra > 1 { "s" } else { "" }), CREAM);
        }
        if pick_up {
            self.sel = Some(Sel::Kind(p.kind));
            self.sel_uid = held;
        }
        fb.play(Snd::Snap.play());
    }

    fn undo(&mut self, s: &mut Run) {
        if let Some((d, inv, plate, bolts)) = self.undo.pop() {
            s.design = d;
            s.inv = inv;
            s.plate = plate;
            s.bolts = bolts;
        }
    }
}

// ------------------------------------------------------------------ setup
#[allow(clippy::too_many_arguments)]
fn spawn_workshop(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    fonts: Res<Fonts>,
    mut s: ResMut<Run>,
    mut toasts: MessageWriter<Toast>,
    mut gizmo_cfg: ResMut<GizmoConfigStore>,
    art: Res<Art>,
) {
    let scope = DespawnOnExit(Screen::Workshop);
    s.testing = false;
    let (gizmos, _) = gizmo_cfg.config_mut::<DefaultGizmoConfigGroup>();
    gizmos.depth_bias = -1.0;
    gizmos.line.width = 2.5;
    // warm garage: steep strip-light "sun", brown bounce, warm point lights under the strips
    scene::sun_lights(&mut commands, SunOpts { size: 4.0, dir: Vec3::new(2.0, 12.0, -3.0), sun: Color::srgb_u8(0xff, 0xe4, 0xc4), intensity: 0.55, sky: Color::srgb_u8(0xb4, 0xb2, 0xb4), ground: Color::srgb_u8(0x6e, 0x60, 0x54) }, scope.clone());
    for p in [Vec3::new(0.0, 3.6, -10.6), Vec3::new(10.6, 3.6, 0.0), Vec3::new(-10.6, 3.6, 0.0), Vec3::new(7.0, 3.6, 10.6), Vec3::new(-7.0, 3.6, 10.6)] {
        commands.spawn((PointLight { color: Color::srgb_u8(0xff, 0xd9, 0xa0), intensity: 60_000.0, range: 8.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(p + Vec3::Y * BENCH_Y), scope.clone()));
    }
    // the garage (art/workshop.py): floor top at BENCH_Y, turntable deck on top
    let strip = models::glow_mat(&mut mats, models::hexc("#fff4dc"), 2.6);
    let day = models::glow_mat(&mut mats, models::hexc("#fff1d0"), 2.0);
    let amber = models::glow_mat(&mut mats, models::hexc("#ffb04a"), 3.0);
    let ring = models::glow_mat(&mut mats, models::hexc("#f2b632"), 1.6);
    let pick = |n: &str| match n {
        "glow_ring" => ring.clone(),
        "glow_door_gap" => day.clone(),
        "glow_lamp" => amber.clone(),
        _ => strip.clone(),
    };
    for a in ["ws_floor", "ws_wall_back", "ws_wall_left", "ws_wall_right", "ws_wall_front", "ws_bench", "ws_props", "ws_corners"] {
        art.spawn_all(&mut commands, &mut meshes, a, &[], Transform::from_xyz(0.0, BENCH_Y, 0.0), pick, scope.clone());
    }
    art.spawn_all(&mut commands, &mut meshes, "ws_turntable", &[], Transform::from_xyz(0.0, BENCH_Y + 0.06, 0.0), pick, (Name::new("Turntable"), scope.clone()));
    commands.spawn((
        scene::camera_bundle(38.0, Transform::default()),
        Camera { clear_color: ClearColorConfig::Custom(BG), ..default() },
        DistanceFog { color: BG, falloff: FogFalloff::Linear { start: 14.0, end: 30.0 }, ..default() },
        WsCam,
        scope.clone(),
    ));
    let ghost_mat = mats.add(StandardMaterial { base_color: Color::srgba(0.61, 1.0, 0.48, 0.45), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
    for i in 0..2 {
        commands.spawn((Name::new("Ghost"), Ghost(i, ghost_mat.clone()), Transform::default(), Visibility::Hidden, scope.clone()));
    }

    // never let a run softlock with no way to move
    if compute_stats(&s.design).drive == 0.0 && !s.inv.iter().any(|x| x.kind.def().category == Category::Drive) {
        s.add_item(PartKind::Leg, 0.0);
        s.add_item(PartKind::Leg, 0.0);
        toasts.write(Toast("The yard owner takes pity and hands you 2 stompy legs".into(), YELLOW));
    }

    // UI skeleton; the columns are filled by the refresh systems
    let col = |left: bool| Node {
        position_type: PositionType::Absolute,
        top: Val::Px(14.0),
        bottom: Val::Px(84.0),
        left: if left { Val::Px(14.0) } else { Val::Auto },
        right: if left { Val::Auto } else { Val::Px(14.0) },
        width: Val::Px(if left { 270.0 } else { 300.0 }),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(10.0),
        overflow: Overflow::clip(),
        ..default()
    };
    commands.spawn((Name::new("Parts column"), col(true), LeftCol, scope.clone()));
    commands.spawn((Name::new("Info column"), col(false), RightCol, scope.clone()));
    commands.spawn((widget::ui_root("Tools", Node { top: Val::Px(14.0), justify_content: JustifyContent::Center, column_gap: Val::Px(8.0), ..default() }), ToolsRow, scope.clone()));
    commands.spawn((widget::ui_root("Go buttons", Node { bottom: Val::Px(16.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, column_gap: Val::Px(14.0), ..default() }), scope.clone())).with_children(|c| {
        c.spawn((widget::button(&fonts, "Test drive", BtnStyle::Teal, 22.0), WsAction::TestDrive));
        c.spawn((widget::button(&fonts, "", BtnStyle::Primary, 26.0), WsAction::Fight, FightLabel));
    });
    commands.spawn((
        Name::new("Tooltip"),
        Node { position_type: PositionType::Absolute, padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
        BackgroundColor(Color::srgba(0.1, 0.09, 0.13, 0.88)),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(50),
        Tip,
        scope,
        children![(widget::text(&fonts, "", 15.0, CREAM), TextLayout::default().with_no_wrap(), Pickable::IGNORE)],
    ));

    // `?yaw=&pitch=&dist=` (degrees, metres) fix the starting view for screenshots
    let pf = |k: &str, d: f32| crate::params::param(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
    commands.insert_resource(Orbit { az: pf("yaw", 40.0).to_radians(), elev: pf("pitch", 28.6).to_radians(), dist: pf("dist", 5.2), press: None });
    commands.insert_resource(Hover::default());
    commands.insert_resource(Tool { sel: None, sel_uid: None, roll: 0.0, tilt: 0.0, mirror: true, undo: vec![] });
}

// ------------------------------------------------------------------ input
fn orbit_camera(
    mut orbit: ResMut<Orbit>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    over_ui: Res<PointerOverUi>,
    win: Single<&Window, With<PrimaryWindow>>,
    mut cam: Single<&mut Transform, With<WsCam>>,
) {
    let dt = time.delta_secs();
    if keys.pressed(KeyCode::KeyQ) {
        orbit.az += dt * 1.8;
    }
    if keys.pressed(KeyCode::KeyE) {
        orbit.az -= dt * 1.8;
    }
    if let Some(c) = win.cursor_position() {
        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            if mouse.just_pressed(button) && !over_ui.0 {
                orbit.press = Some(Press { button, moved: 0.0, last: c });
            }
        }
        let o = &mut *orbit;
        if let Some(p) = o.press.as_mut().filter(|p| mouse.pressed(p.button)) {
            let d = c - p.last;
            p.moved += d.x.abs() + d.y.abs();
            if p.moved > DRAG_PX {
                o.az -= d.x * 0.008;
                o.elev = (o.elev + d.y * 0.006).clamp(0.05, 1.4);
            }
            p.last = c;
        }
    }
    if !over_ui.0 && scroll.delta.y != 0.0 {
        let k = if scroll.unit == MouseScrollUnit::Line { 0.4 } else { 0.004 };
        orbit.dist = (orbit.dist - scroll.delta.y * k).clamp(2.8, 10.0);
    }
    let target = Vec3::new(0.0, BENCH_Y + 0.4, 0.0);
    let p = target + Vec3::new(orbit.az.sin() * orbit.elev.cos(), orbit.elev.sin(), orbit.az.cos() * orbit.elev.cos()) * orbit.dist;
    **cam = Transform::from_translation(p).looking_at(target, Vec3::Y);
}

fn shortcut_keys(keys: Res<ButtonInput<KeyCode>>, mut tool: ResMut<Tool>, mut s: ResMut<Run>, mut fb: Feedback) {
    for (k, a) in [(KeyCode::KeyM, WsAction::Mirror), (KeyCode::KeyR, WsAction::Roll), (KeyCode::KeyT, WsAction::Tilt), (KeyCode::KeyZ, WsAction::Undo)] {
        if keys.just_pressed(k) {
            tool_action(a, &mut tool, &mut s);
            fb.play(Snd::Click.play());
        }
    }
    if keys.just_pressed(KeyCode::Escape) && tool.sel.is_some() {
        tool.sel = None;
    }
}

/// Mirror / roll / tilt / undo, shared by the buttons and their shortcuts.
fn tool_action(a: WsAction, tool: &mut Tool, s: &mut Run) {
    match a {
        WsAction::Mirror => tool.mirror = !tool.mirror,
        WsAction::Roll => tool.roll = (tool.roll + FRAC_PI_4) % TAU,
        WsAction::Tilt => tool.tilt = TILTS[(TILTS.iter().position(|x| *x == tool.tilt).unwrap_or(0) + 1) % TILTS.len()],
        WsAction::Undo => tool.undo(s),
        _ => {}
    }
}

/// Ray-pick the part under the cursor (against each part's bounds box, which
/// also tells us which face we're on) and build the placement plan.
fn pick(
    mut hover: ResMut<Hover>,
    tool: Res<Tool>,
    orbit: Res<Orbit>,
    s: Res<Run>,
    over_ui: Res<PointerOverUi>,
    win: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform), With<WsCam>>,
) {
    hover.part = None;
    hover.plan = None;
    // once a press turns into an orbit drag stop picking, so a click keeps its target
    if over_ui.0 || orbit.dragging() {
        return;
    }
    let (camera, gt) = *cam;
    let Some(ray) = win.cursor_position().and_then(|c| camera.viewport_to_world(gt, c).ok()) else { return };
    let root = Vec3::new(0.0, bot_y(&s.design), 0.0);
    let mut best = f32::INFINITY;
    let mut face_hit = None;
    for p in &s.design.parts {
        let b = p.kind.def().bounds;
        let inv = p.rot.inverse();
        let o = inv * (ray.origin - root - p.pos);
        let d = inv * *ray.direction;
        let (mut t_in, mut t_out, mut axis, mut sign) = (f32::NEG_INFINITY, f32::INFINITY, 0usize, 1.0f32);
        for a in 0..3 {
            let (lo, hi) = (b.c[a] - b.h[a] - 0.01, b.c[a] + b.h[a] + 0.01);
            if d[a].abs() < 1e-9 {
                if o[a] < lo || o[a] > hi {
                    t_in = f32::INFINITY;
                }
                continue;
            }
            let (t1, t2) = ((lo - o[a]) / d[a], (hi - o[a]) / d[a]);
            if t1.min(t2) > t_in {
                t_in = t1.min(t2);
                axis = a;
                sign = if t1 < t2 { -1.0 } else { 1.0 };
            }
            t_out = t_out.min(t1.max(t2));
        }
        if t_in <= t_out && t_in > 0.0 && t_in < best {
            best = t_in;
            let lp = o + d * t_in;
            let others: Vec<usize> = (0..3).filter(|x| *x != axis).collect();
            let snap = |k: usize| (((lp[k] - b.c[k]) / SNAP).round() * SNAP).clamp(-b.h[k], b.h[k]);
            let face = match (axis, sign > 0.0) {
                (0, true) => Face::PX,
                (0, false) => Face::NX,
                (1, true) => Face::PY,
                (1, false) => Face::NY,
                (_, true) => Face::PZ,
                _ => Face::NZ,
            };
            face_hit = Some((p.id, face, snap(others[0]), snap(others[1])));
        }
    }
    if let Some((id, face, u, v)) = face_hit {
        hover.part = Some(id);
        if let Some(k) = tool.kind() {
            let par = s.design.by_id(id).unwrap();
            hover.plan = Some(tool.make_plan(&s, k, par, face, u, v));
        }
    }
}

/// Clicks on the bench (a press that didn't turn into a drag): place,
/// pick up or unbolt.
fn handle_clicks(mouse: Res<ButtonInput<MouseButton>>, mut orbit: ResMut<Orbit>, hover: Res<Hover>, mut tool: ResMut<Tool>, mut s: ResMut<Run>, mut fb: Feedback) {
    let Some(press) = orbit.press.as_ref().filter(|p| mouse.just_released(p.button)) else { return };
    let (button, dragged) = (press.button, press.moved > DRAG_PX);
    orbit.press = None;
    if dragged || button == MouseButton::Middle {
        return;
    }
    if button == MouseButton::Right {
        if tool.sel.is_some() {
            tool.sel = None;
        } else if let Some(h) = hover.part {
            tool.remove(&mut s, h, false, &mut fb);
        }
        return;
    }
    match tool.sel {
        Some(Sel::Remove) => {
            if let Some(h) = hover.part {
                tool.remove(&mut s, h, false, &mut fb);
            }
        }
        Some(Sel::Kind(_)) => {
            if let Some(plan) = &hover.plan {
                tool.place(&mut s, plan, &mut fb);
            }
        }
        None => {
            if let Some(h) = hover.part.filter(|h| s.design.by_id(*h).is_some_and(|p| p.kind != PartKind::Core)) {
                tool.remove(&mut s, h, true, &mut fb);
            }
        }
    }
}

/// Every workshop button routes here through its [`WsAction`]. `If` skips
/// it on other screens, where there is no `Tool`.
fn on_action(ev: On<Activate>, actions: Query<&WsAction>, mut tool: If<ResMut<Tool>>, mut s: ResMut<Run>, mut fb: Feedback, mut next: ResMut<NextState<Screen>>) {
    let Ok(&action) = actions.get(ev.entity) else { return };
    match action {
        WsAction::TestDrive | WsAction::Fight => {
            if validate(&s, &mut fb) {
                s.testing = matches!(action, WsAction::TestDrive);
                next.set(Screen::Fight);
            }
        }
        WsAction::Mirror | WsAction::Roll | WsAction::Tilt | WsAction::Undo => {
            tool_action(action, &mut tool, &mut s);
            fb.play(Snd::Click.play());
        }
        WsAction::Unbolt => tool.sel = if tool.sel == Some(Sel::Remove) { None } else { Some(Sel::Remove) },
        WsAction::Kind(k) => {
            tool.sel = if tool.sel == Some(Sel::Kind(k)) { None } else { Some(Sel::Kind(k)) };
            tool.sel_uid = None;
        }
        WsAction::Item(uid) => {
            if let Some(it) = s.inv.iter().find(|x| x.uid == uid) {
                tool.sel = Some(Sel::Kind(it.kind));
                tool.sel_uid = Some(it.uid);
            }
        }
        WsAction::FixSelected => {
            let Some(k) = tool.kind() else { return };
            let Some(uid) = tool.pick(&s, k).map(|x| x.uid) else { return };
            tool.snapshot(&s);
            let st = &mut *s;
            let it = st.inv.iter_mut().find(|x| x.uid == uid).unwrap();
            match fix(&mut st.plate, &mut st.bolts, k, &mut it.dmg) {
                None => {
                    tool.undo.pop();
                    fb.toast("Not enough plate or bolts. Time for the scrapheap...", RED);
                }
                Some(full) => {
                    fb.play(Snd::Place.play());
                    fb.play(Snd::Ding.play());
                    let name = k.def().name;
                    fb.toast(if full { format!("{name} good as new") } else { format!("Patched the {name} up a bit") }, GREEN);
                }
            }
        }
        WsAction::RepairAll => {
            tool.snapshot(&s);
            let st = &mut *s;
            let mut order: Vec<usize> = (0..st.design.parts.len()).filter(|&i| st.design.parts[i].dmg > 0.01).collect();
            order.sort_by(|&a, &b| st.design.parts[b].dmg.total_cmp(&st.design.parts[a].dmg));
            let (mut fixed, mut partial) = (0, 0);
            for i in order {
                let p = &mut st.design.parts[i];
                match fix(&mut st.plate, &mut st.bolts, p.kind, &mut p.dmg) {
                    Some(true) => fixed += 1,
                    Some(false) => partial += 1,
                    None => {}
                }
            }
            if fixed + partial > 0 {
                fb.play(Snd::Place.play());
                fb.play(Snd::Ding.play());
                let more = if partial > 0 { format!(", patched {partial} more") } else { String::new() };
                fb.toast(format!("Repaired {fixed} part{}{more}", if fixed == 1 { "" } else { "s" }), GREEN);
            } else {
                tool.undo.pop();
                fb.toast("Not enough plate or bolts. Time for the scrapheap...", RED);
            }
        }
        WsAction::Fabricate(n) => {
            let (k, c) = FAB_COST[n];
            if s.plate >= c {
                tool.snapshot(&s);
                s.plate -= c;
                s.add_item(k, 0.0);
                fb.play(Snd::Place.play());
                fb.toast(format!("Welded a {}", k.def().name), CREAM);
            }
        }
        WsAction::Cheat => {
            // testing shortcut: 10 fresh of every part, plus plate and bolts
            tool.snapshot(&s);
            for k in KIND_ORDER {
                for _ in 0..10 {
                    s.add_item(k, 0.0);
                }
            }
            s.plate += 30;
            s.bolts += 30;
            fb.play(Snd::Ding.play());
            fb.toast("Test stash: +10 of every part, +30 plate and bolts", GREEN);
        }
        WsAction::Opponent(n) => {
            // dev: jump straight to any opponent
            s.round = n;
            fb.play(Snd::Ding.play());
        }
        WsAction::Paint(n) => s.design.paint = Color::Srgba(Srgba::hex(PAINTS[n]).unwrap()),
    }
}

// ------------------------------------------------------------------ 3D feedback
fn rebuild_bot(mut commands: Commands, s: Res<Run>, old: Query<Entity, With<WsBot>>, mut meshes: ResMut<Assets<Mesh>>, art: Res<Art>, eyes: Res<EyeMats>) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let e = spawn_preview(&mut commands, &mut meshes, &art, &eyes, &s.design, true, Transform::from_xyz(0.0, bot_y(&s.design), 0.0), DespawnOnExit(Screen::Workshop));
    commands.entity(e).insert(WsBot);
}

/// Translucent preview of where the part(s) would go, red when blocked.
fn show_ghosts(
    hover: Res<Hover>,
    s: Res<Run>,
    time: Res<Time>,
    mut key: Local<Option<(PartKind, [i32; 4], [u8; 4])>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    art: Res<Art>,
    eyes: Res<EyeMats>,
    mut q: Query<(Entity, &Ghost, &mut Transform, &mut Visibility)>,
) {
    let root = Vec3::new(0.0, bot_y(&s.design), 0.0);
    let plan = hover.plan.as_ref();
    // rebuild the meshes when the kind, orientation or paint change (legs pose by orientation)
    let k = plan.map(|p| (p.kind, p.a.rot.to_array().map(|x| (x * 100.0) as i32), s.design.paint.to_srgba().to_u8_array()));
    let rebuild = k != *key;
    *key = k;
    let xs = [plan.map(|p| p.a), plan.and_then(|p| p.b.map(|b| b.0))];
    for (ge, g, mut tf, mut vis) in &mut q {
        let x = xs[g.0];
        *vis = if x.is_some() { Visibility::Visible } else { Visibility::Hidden };
        let (Some(x), Some(p)) = (x, plan) else { continue };
        if rebuild {
            commands.entity(ge).despawn_related::<Children>();
            let pl = Placed { id: 0, kind: p.kind, parent: None, pos: x.pos, rot: x.rot, dmg: 0.0, mirror: None };
            let look = PartLook { ghost: Some(g.1.clone()), ..part_look(s.design.paint, &eyes) };
            let e = spawn_static_part(&mut commands, &mut meshes, &art, &pl, &look, Transform::IDENTITY, bevy::light::NotShadowCaster);
            commands.entity(ge).add_child(e);
        }
        *tf = Transform { translation: root + x.pos, rotation: x.rot, ..default() };
        if let Some(mut m) = mats.get_mut(&g.1) {
            let a = 0.4 + (time.elapsed_secs() * 8.0).sin() * 0.1;
            m.base_color = if p.why.is_some() { Color::srgba(1.0, 0.35, 0.29, a) } else { Color::srgba(0.61, 1.0, 0.48, a) };
        }
    }
}

/// Outline the part you'd pick up / unbolt.
fn hover_outline(hover: Res<Hover>, tool: Res<Tool>, s: Res<Run>, mut gizmos: Gizmos) {
    if tool.kind().is_some() {
        return;
    }
    let Some(p) = hover.part.and_then(|id| s.design.by_id(id)) else { return };
    let b = p.kind.def().bounds;
    let root = Vec3::new(0.0, bot_y(&s.design), 0.0);
    let col = if tool.sel == Some(Sel::Remove) { Color::srgb_u8(0xff, 0x9a, 0x4a) } else { CREAM };
    gizmos.cube(Transform { translation: root + p.pos + p.rot * b.c, rotation: p.rot, scale: b.h * 2.0 + Vec3::splat(0.04) }, col);
}

fn cursor_tip(hover: Res<Hover>, tool: Res<Tool>, s: Res<Run>, win: Single<&Window, With<PrimaryWindow>>, tip: Single<(&mut Node, &mut Visibility, &Children), With<Tip>>, mut texts: Query<&mut Text>) {
    let (mut node, mut vis, ch) = tip.into_inner();
    let kind = tool.kind();
    let msg = if let (Some(k), Some(p)) = (kind, &hover.plan) {
        match &p.why {
            Some(w) => format!("x  {w}"),
            None => {
                let pct = tool.pick(&s, k).map_or(100, |it| ((1.0 - it.dmg) * 100.0).round() as i32);
                format!("{}{}{}", k.def().name, if pct < 100 { format!(" ({pct}%)") } else { String::new() }, if p.b.is_some() { "  x2 mirrored" } else { "" })
            }
        }
    } else if let Some(k) = kind {
        format!("Point at your bot to place a {}", k.def().name)
    } else if let Some(p) = hover.part.and_then(|h| s.design.by_id(h)) {
        let hp = ((1.0 - p.dmg) * 100.0).round();
        let (res, n) = state::repair_cost(p.kind, p.dmg);
        let fixs = if n > 0 { format!(" · fix: {n} {}", if res == Stock::Plate { "plate" } else { "bolts" }) } else { String::new() };
        let hint = if p.kind != PartKind::Core { if tool.sel == Some(Sel::Remove) { " · click to unbolt" } else { " · click to move · right-click to unbolt" } } else { "" };
        format!("{} · {hp}% hp{fixs}{hint}", p.kind.def().name)
    } else {
        String::new()
    };
    let cursor = win.cursor_position();
    vis.set_if_neq(if msg.is_empty() || cursor.is_none() { Visibility::Hidden } else { Visibility::Visible });
    if let Some(c) = cursor {
        node.left = Val::Px(c.x + 16.0);
        node.top = Val::Px(c.y - 34.0);
    }
    for c in ch.iter() {
        if let Ok(mut t) = texts.get_mut(c)
            && t.0 != msg {
                t.0 = msg.clone();
            }
    }
}

// ------------------------------------------------------------------ panels
fn section(fonts: &Fonts, s: &str) -> impl Bundle {
    (widget::title_text(fonts, s.to_uppercase(), 15.0, TEAL), Pickable::IGNORE)
}

/// A small button; `disabled` greys it out.
fn small_button(c: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, style: BtnStyle, size: f32, disabled: bool, action: WsAction) {
    let mut e = c.spawn((widget::button(fonts, label, style, size), action));
    if disabled {
        e.insert(InteractionDisabled);
    }
}

fn wear_col(pct: i32) -> Color {
    if pct >= 70 { GREEN } else if pct >= 40 { YELLOW } else { RED }
}

fn swatch(color: Color) -> impl Bundle {
    (Node { width: Val::Px(14.0), height: Val::Px(14.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, BackgroundColor(color), Pickable::IGNORE)
}

/// Parts bin + description of the selection.
fn refresh_left(mut commands: Commands, tool: Res<Tool>, s: Res<Run>, fonts: Res<Fonts>, col: Single<Entity, With<LeftCol>>) {
    let fonts = &*fonts;
    let sel_bg = YELLOW;
    let row_bg = Color::srgba(1.0, 1.0, 1.0, 0.05);
    commands.entity(*col).despawn_related::<Children>().with_children(|col| {
        col.spawn(widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), flex_shrink: 1.0, overflow: Overflow::clip(), ..default() })).with_children(|p| {
            p.spawn(section(fonts, "Parts bin"));
            let on = tool.sel == Some(Sel::Remove);
            p.spawn((
                widget::clickable(BtnStyle::Custom(if on { sel_bg } else { row_bg }), Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), column_gap: Val::Px(8.0), ..default() }),
                WsAction::Unbolt,
                children![swatch(Color::srgb_u8(0x5b, 0x56, 0x6a)), (widget::text(fonts, "Unbolt tool", 15.0, if on { INK } else { CREAM }), Pickable::IGNORE)],
            ));
            for k in KIND_ORDER {
                let d = k.def();
                let mut items: Vec<&InvItem> = s.inv.iter().filter(|x| x.kind == k).collect();
                items.sort_by(|a, b| a.dmg.total_cmp(&b.dmg));
                let n = items.len();
                let on = tool.sel == Some(Sel::Kind(k));
                let fg = if on { INK } else if n == 0 { Color::srgba(1.0, 0.95, 0.86, 0.35) } else { CREAM };
                p.spawn((
                    widget::clickable(BtnStyle::Custom(if on { sel_bg } else { row_bg }), Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)), column_gap: Val::Px(8.0), ..default() }),
                    WsAction::Kind(k),
                    children![
                        swatch(d.colour.with_alpha(if n == 0 { 0.35 } else { 1.0 })),
                        (widget::text(fonts, d.name, 15.0, fg), Node { flex_grow: 1.0, ..default() }, Pickable::IGNORE),
                        (widget::title_text(fonts, format!("{n}"), 15.0, fg), Pickable::IGNORE),
                    ],
                ));
                // the actual roster: shown when it's worth choosing (some worn) or selected
                if n == 0 || !(on || items.iter().any(|x| x.dmg > 0.01)) {
                    continue;
                }
                let cur = if on { tool.pick(&s, k).map(|x| x.uid) } else { None };
                let fresh: Vec<&InvItem> = items.iter().copied().filter(|x| x.dmg <= 0.01).collect();
                let mut groups: Vec<Vec<&InvItem>> = vec![];
                if !fresh.is_empty() {
                    groups.push(fresh);
                }
                groups.extend(items.iter().filter(|x| x.dmg > 0.01).map(|x| vec![*x]));
                p.spawn((Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), padding: UiRect::new(Val::Px(30.0), Val::Px(0.0), Val::Px(1.0), Val::Px(3.0)), ..default() }, Pickable::IGNORE)).with_children(|chips| {
                    for g in groups {
                        let pct = ((1.0 - g[0].dmg) * 100.0).round() as i32;
                        let sel = cur.is_some_and(|c| g.iter().any(|x| x.uid == c));
                        let uid = if sel { cur.unwrap() } else { g[0].uid };
                        let label = if g.len() > 1 { format!("{pct}% x{}", g.len()) } else { format!("{pct}%") };
                        chips.spawn((
                            widget::clickable(
                                BtnStyle::Custom(if sel { Color::srgb_u8(0x5a, 0x55, 0x66) } else { Color::srgba(0.0, 0.0, 0.0, 0.25) }),
                                Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)), column_gap: Val::Px(5.0), border: UiRect::all(Val::Px(if sel { 2.0 } else { 0.0 })), ..default() },
                            ),
                            WsAction::Item(uid),
                            children![
                                (
                                    Node { width: Val::Px(22.0), height: Val::Px(5.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() },
                                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12)),
                                    Pickable::IGNORE,
                                    children![(
                                        Node { width: Val::Percent(pct as f32), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() },
                                        BackgroundColor(wear_col(pct)),
                                        Pickable::IGNORE,
                                    )],
                                ),
                                (widget::text(fonts, label, 13.0, CREAM), Pickable::IGNORE),
                            ],
                        ));
                    }
                });
            }
        });
        col.spawn(widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), flex_shrink: 0.0, ..default() })).with_children(|p| match tool.sel {
            Some(Sel::Kind(k)) => {
                let d = k.def();
                p.spawn(widget::title_text(fonts, d.name, 18.0, YELLOW));
                p.spawn(widget::text(fonts, format!("{}kg · {}{} power", d.mass, if d.power > 0.0 { "+" } else { "" }, d.power), 14.0, Color::srgb_u8(0xd9, 0xcc, 0xb4)));
                p.spawn(widget::text(fonts, d.desc, 14.0, CREAM));
                if let Some(it) = tool.pick(&s, k).filter(|it| it.dmg > 0.01) {
                    let (res, n) = state::repair_cost(k, it.dmg);
                    let have = if res == Stock::Plate { s.plate } else { s.bolts };
                    p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                        let pct = ((1.0 - it.dmg) * 100.0).round() as i32;
                        r.spawn(widget::text(fonts, format!("Condition {pct}%"), 14.0, wear_col(pct)));
                        small_button(r, fonts, &format!("Fix: {n} {}", if res == Stock::Plate { "plate" } else { "bolts" }), BtnStyle::Secondary, 14.0, have <= 0, WsAction::FixSelected);
                    });
                }
            }
            Some(Sel::Remove) => {
                p.spawn(widget::text(fonts, "Click any part to unbolt it (and everything attached to it) back into the bin.", 14.0, CREAM));
            }
            None => {
                p.spawn(widget::text(fonts, "Pick a part, then click any face of your bot to bolt it on. Click a fitted part to pick it up and move it. Drag to orbit, scroll to zoom.", 14.0, CREAM));
            }
        });
    });
}

/// Opponent, stats, stash and paint.
fn refresh_right(mut commands: Commands, s: Res<Run>, fonts: Res<Fonts>, col: Single<Entity, With<RightCol>>) {
    let fonts = &*fonts;
    let st = compute_stats(&s.design);
    let opps = state::opponents();
    let opp = &opps[s.round.min(opps.len() - 1)];
    let damaged: Vec<&Placed> = s.design.parts.iter().filter(|c| c.dmg > 0.01).collect();
    let (mut plate_need, mut bolt_need) = (0, 0);
    for c in &damaged {
        let (r, n) = state::repair_cost(c.kind, c.dmg);
        if r == Stock::Plate { plate_need += n } else { bolt_need += n }
    }
    commands.entity(*col).despawn_related::<Children>().with_children(|col| {
        col.spawn(widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() })).with_children(|p| {
            p.spawn(section(fonts, &format!("Round {} of {} · vs {}", s.round + 1, opps.len(), opp.design.name)));
            p.spawn(Node { column_gap: Val::Px(5.0), align_items: AlignItems::Center, ..default() }).with_children(|l| {
                // dev: click a round to jump straight to that opponent
                for i in 0..opps.len() {
                    let style = if i < s.round { Color::srgba(0.486, 0.812, 0.353, 0.3) } else if i == s.round { YELLOW } else { Color::srgba(1.0, 1.0, 1.0, 0.08) };
                    l.spawn((
                        widget::clickable(BtnStyle::Custom(style), Node { width: Val::Px(28.0), height: Val::Px(24.0), justify_content: JustifyContent::Center, ..default() }),
                        WsAction::Opponent(i),
                        children![(widget::title_text(fonts, format!("{}", i + 1), 15.0, if i == s.round { INK } else { CREAM }), Pickable::IGNORE)],
                    ));
                }
                l.spawn(widget::text(fonts, format!("  lives {}", s.lives), 14.0, RED));
            });
            p.spawn(widget::text(fonts, opp.blurb, 14.0, CREAM));
        });
        col.spawn(widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(5.0), ..default() })).with_children(|p| {
            p.spawn(section(fonts, &s.design.name));
            for (name, v, c) in [("Speed", st.speed, YELLOW), ("Armour", st.armour, TEAL), ("Bite", st.bite, RED), ("Balance", st.balance, Color::srgb_u8(0xa8, 0x8c, 0xf0))] {
                p.spawn((Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }, children![widget::text(fonts, name, 15.0, CREAM), widget::bars(v, c)]));
            }
            let pw = if st.power_ratio < 1.0 { format!(" ({}%)", (st.power_ratio * 100.0).round()) } else { String::new() };
            p.spawn(widget::text(fonts, format!("{:.1} kg · {} parts · power {}/{}{pw}", st.mass, s.design.parts.len(), st.supply, st.demand), 14.0, Color::srgb_u8(0xd9, 0xcc, 0xb4)));
            for w in &st.warnings {
                p.spawn(widget::text(fonts, format!("! {w}"), 14.0, Color::srgb_u8(0xff, 0x9a, 0x6a)));
            }
        });
        col.spawn(widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), ..default() })).with_children(|p| {
            p.spawn(section(fonts, "Stash"));
            p.spawn(widget::title_text(fonts, format!("Bolts {}    Plate {}", s.bolts, s.plate), 17.0, CREAM));
            if damaged.is_empty() {
                p.spawn(widget::text(fonts, "Everything is in one piece", 14.0, GREEN));
            } else {
                p.spawn(widget::text(fonts, format!("{} damaged part{} · needs {plate_need} plate, {bolt_need} bolts", damaged.len(), if damaged.len() > 1 { "s" } else { "" }), 14.0, CREAM));
            }
            p.spawn(Node { column_gap: Val::Px(5.0), row_gap: Val::Px(5.0), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|r| {
                small_button(r, fonts, "Repair", BtnStyle::Secondary, 15.0, damaged.is_empty(), WsAction::RepairAll);
                for (i, (k, c)) in FAB_COST.iter().enumerate() {
                    small_button(r, fonts, &format!("+{} {c}p", k.def().name), BtnStyle::Secondary, 12.0, s.plate < *c, WsAction::Fabricate(i));
                }
            });
            small_button(p, fonts, "Test: +10 of every part", BtnStyle::Secondary, 13.0, false, WsAction::Cheat);
        });
        col.spawn(widget::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() })).with_children(|p| {
            p.spawn(section(fonts, "Paint"));
            p.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                for (i, h) in PAINTS.iter().enumerate() {
                    let c = Color::Srgba(Srgba::hex(h).unwrap());
                    let on = s.design.paint.to_srgba().to_hex() == c.to_srgba().to_hex();
                    r.spawn((widget::clickable(BtnStyle::Custom(c), Node { width: Val::Px(26.0), height: Val::Px(26.0), border: UiRect::all(Val::Px(if on { 3.0 } else { 0.0 })), ..default() }), WsAction::Paint(i)));
                }
            });
        });
    });
}

fn refresh_tools(mut commands: Commands, tool: Res<Tool>, fonts: Res<Fonts>, row: Single<Entity, With<ToolsRow>>) {
    let fonts = &*fonts;
    let (deg, tdeg) = (tool.roll.to_degrees().round(), tool.tilt.to_degrees().round());
    commands.entity(*row).despawn_related::<Children>().with_children(|r| {
        small_button(r, fonts, &format!("Mirror {} [M]", if tool.mirror { "on" } else { "off" }), if tool.mirror { BtnStyle::Teal } else { BtnStyle::Secondary }, 15.0, false, WsAction::Mirror);
        small_button(r, fonts, &format!("Roll {deg}° [R]"), BtnStyle::Secondary, 15.0, false, WsAction::Roll);
        small_button(r, fonts, &format!("Tilt {tdeg}° [T]"), BtnStyle::Secondary, 15.0, false, WsAction::Tilt);
        small_button(r, fonts, "Undo [Z]", BtnStyle::Secondary, 15.0, tool.undo.is_empty(), WsAction::Undo);
    });
}

fn update_fight_label(s: Res<Run>, label: Single<&Children, With<FightLabel>>, mut texts: Query<&mut Text>) {
    let opps = state::opponents();
    let name = &opps[s.round.min(opps.len() - 1)].design.name;
    for c in label.iter() {
        if let Ok(mut t) = texts.get_mut(c) {
            t.0 = format!("Fight {name}!");
        }
    }
}
