//! The workshop: KSP-style freeform building on a turntable. Parts snap onto
//! any face of a structural part; roll/tilt/mirror before you bolt them on.
use crate::models::{self, Mats};
use crate::parts::{compute_stats, face_point, mirror_xf, orient_on, Face, Placed, PartKind, Res as Stock, KIND_ORDER};
use crate::preview::{part_geo, spawn_preview};
use crate::scene::{self, SunOpts};
use crate::state::{self, GameState, InvItem, FAB_COST};
use crate::ui::{self, Btn, BtnStyle, Clicked, Fonts, PointerOverUi, Toast};
use crate::{Scoped, Screen};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

const BENCH_Y: f32 = 0.9;
const SNAP: f32 = 0.05;
const TILTS: [f32; 4] = [0.0, 0.4, 0.8, -0.4];
const PAINTS: [&str; 8] = ["#e2483d", "#f2b632", "#3f7fd8", "#3fa7a3", "#7c5bb5", "#5a8a3a", "#ee7a32", "#e8e0d0"];
const BG: Color = Color::srgb(0.227, 0.216, 0.267); // #3a3744

pub struct WorkshopPlugin;
impl Plugin for WorkshopPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Workshop), enter)
            .add_systems(OnExit(Screen::Workshop), |mut commands: Commands| commands.remove_resource::<Ws>())
            .add_systems(Update, (camera, keys, clicks, pick, rebuild_bot, ghosts, refresh_ui, tip).chain().run_if(in_state(Screen::Workshop)));
    }
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

struct Plan {
    kind: PartKind,
    why: Option<String>,
    a: Xf,
    a_parent: u32,
    b: Option<(Xf, u32)>,
}

type Snap = (crate::parts::Design, Vec<InvItem>, i32, i32);

#[derive(Resource)]
struct Ws {
    sel: Option<Sel>,
    /// which bin item of the selected kind to bolt on next (None = healthiest)
    sel_uid: Option<u32>,
    roll: f32,
    tilt: f32,
    mirror: bool,
    az: f32,
    elev: f32,
    dist: f32,
    down: Option<MouseButton>,
    moved: f32,
    last: Vec2,
    hovered: Option<u32>,
    plan: Option<Plan>,
    undo: Vec<Snap>,
    bot_dirty: bool,
    ui_dirty: bool,
    ghost_key: Option<(PartKind, [i32; 4])>,
    ghosts: [Entity; 2],
    ghost_mat: Handle<StandardMaterial>,
    bot: Option<Entity>,
    bot_y: f32,
}

#[derive(Component)]
struct WsCam;
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

impl Ws {
    fn snapshot(&mut self, s: &GameState) {
        self.undo.push((s.design.clone(), s.inv.clone(), s.plate, s.bolts));
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
    }
    fn dirty(&mut self) {
        self.bot_dirty = true;
        self.ui_dirty = true;
    }
    /// The bin item the next placement uses: the chosen one, else the healthiest.
    fn pick<'a>(&self, s: &'a GameState, kind: PartKind) -> Option<&'a InvItem> {
        let mut items: Vec<&InvItem> = s.inv.iter().filter(|x| x.kind == kind).collect();
        if let Some(it) = items.iter().find(|x| Some(x.uid) == self.sel_uid) {
            return Some(it);
        }
        items.sort_by(|a, b| a.dmg.total_cmp(&b.dmg));
        items.first().copied()
    }
    fn take(&self, s: &mut GameState, kind: PartKind) -> f32 {
        let Some(uid) = self.pick(s, kind).map(|x| x.uid) else { return 0.0 };
        let i = s.inv.iter().position(|x| x.uid == uid).unwrap();
        s.inv.remove(i).dmg
    }

    fn make_plan(&self, s: &GameState, kind: PartKind, par: &Placed, face: Face, u: f32, v: f32) -> Plan {
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

    fn place(&mut self, s: &mut GameState, toast: &mut MessageWriter<Toast>) {
        let Some(p) = self.plan.take() else { return };
        if let Some(w) = &p.why {
            toast.write(Toast(w.clone(), ui::RED));
            crate::audio::click();
            self.plan = Some(p);
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
        crate::audio::place();
        self.sel_uid = None;
        if s.count_of(p.kind) == 0 {
            self.sel = None;
        }
        self.dirty();
    }

    fn remove(&mut self, s: &mut GameState, id: u32, pick_up: bool, toast: &mut MessageWriter<Toast>) {
        let Some(p) = s.design.by_id(id).cloned() else { return };
        if p.kind == PartKind::Core {
            toast.write(Toast("The Brain Box stays put.".into(), ui::RED));
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
            toast.write(Toast(format!("{extra} part{} attached to it came off too", if extra > 1 { "s" } else { "" }), ui::CREAM));
        }
        if pick_up {
            self.sel = Some(Sel::Kind(p.kind));
            self.sel_uid = held;
        }
        crate::audio::snap();
        self.dirty();
    }

    fn tool(&mut self, t: &str, s: &mut GameState) {
        match t {
            "m" => self.mirror = !self.mirror,
            "r" => self.roll = (self.roll + std::f32::consts::FRAC_PI_4) % std::f32::consts::TAU,
            "t" => self.tilt = TILTS[(TILTS.iter().position(|x| *x == self.tilt).unwrap_or(0) + 1) % TILTS.len()],
            "z" => {
                if let Some((d, inv, plate, bolts)) = self.undo.pop() {
                    s.design = d;
                    s.inv = inv;
                    s.plate = plate;
                    s.bolts = bolts;
                    self.bot_dirty = true;
                }
            }
            _ => {}
        }
        crate::audio::click();
        self.ui_dirty = true;
    }
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

fn validate(s: &GameState, toast: &mut MessageWriter<Toast>) -> bool {
    if compute_stats(&s.design).drive == 0.0 {
        toast.write(Toast("Your bot needs legs or wheels that touch the floor!".into(), ui::RED));
        return false;
    }
    true
}

// ------------------------------------------------------------------ setup
fn enter(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shared: Res<Mats>,
    fonts: Res<Fonts>,
    mut s: ResMut<GameState>,
    mut toast: MessageWriter<Toast>,
    mut gizmo_cfg: ResMut<GizmoConfigStore>,
) {
    s.testing = false;
    gizmo_cfg.config_mut::<DefaultGizmoConfigGroup>().0.depth_bias = -1.0;
    gizmo_cfg.config_mut::<DefaultGizmoConfigGroup>().0.line.width = 2.5;
    scene::sun_lights(&mut commands, SunOpts { size: 4.0, dir: Vec3::new(4.0, 10.0, -5.0), intensity: 1.0, sky: Color::srgb_u8(0xe8, 0xee, 0xf5), ground: Color::srgb_u8(0x8a, 0x80, 0x90), ..default() }, Scoped);
    // plain studio: a floor and the turntable, nothing in the way
    models::reseed(11);
    let mut env = models::cyl(40.0, 40.0, 0.2, 32, models::hexc("#4a4655"), 0.0).transformed(Transform::from_xyz(0.0, BENCH_Y - 0.1, 0.0));
    env.add(&models::cyl(1.75, 1.8, 0.06, 28, models::hexc("#5c5868"), 0.0), Transform::from_xyz(0.0, BENCH_Y + 0.03, 0.0));
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        let c = if i % 3 != 0 { models::hexc("#7a7586") } else { models::Pal::yellow() };
        env.add(&models::bx(0.04, 0.012, 0.18, c, 0.0, 0.0), Transform::from_xyz(a.sin() * 1.6, BENCH_Y + 0.065, a.cos() * 1.6).with_rotation(Quat::from_rotation_y(a)));
    }
    // "front" marker so you know which way the bot faces
    env.add(&models::cyl(0.0, 0.22, 0.05, 3, models::Pal::yellow(), 0.0), Transform::from_xyz(0.0, BENCH_Y + 0.07, 1.95).with_rotation(Quat::from_rotation_y(std::f32::consts::PI)));
    commands.spawn((Mesh3d(meshes.add(env.to_mesh())), MeshMaterial3d(shared.vc.clone()), Scoped));
    commands.spawn((
        scene::camera_bundle(38.0, Transform::default()),
        Camera { clear_color: ClearColorConfig::Custom(BG), ..default() },
        DistanceFog { color: BG, falloff: FogFalloff::Linear { start: 14.0, end: 30.0 }, ..default() },
        WsCam,
        Scoped,
    ));
    let ghost_mat = mats.add(StandardMaterial { base_color: Color::srgba(0.61, 1.0, 0.48, 0.45), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
    let ghosts = [0, 1].map(|_| commands.spawn((Mesh3d::default(), MeshMaterial3d(ghost_mat.clone()), Transform::default(), Visibility::Hidden, Scoped)).id());

    // never let a run softlock with no way to move
    if compute_stats(&s.design).drive == 0.0 && !s.inv.iter().any(|x| x.kind.def().category == crate::parts::Category::Drive) {
        s.add_item(PartKind::Leg, 0.0);
        s.add_item(PartKind::Leg, 0.0);
        toast.write(Toast("The yard owner takes pity and hands you 2 stompy legs".into(), ui::YELLOW));
    }

    // UI skeleton; the columns are filled by refresh_ui
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
    commands.spawn((col(true), LeftCol, Scoped));
    commands.spawn((col(false), RightCol, Scoped));
    commands.spawn((
        Node { position_type: PositionType::Absolute, top: Val::Px(14.0), left: Val::Px(0.0), right: Val::Px(0.0), justify_content: JustifyContent::Center, column_gap: Val::Px(8.0), ..default() },
        Pickable::IGNORE,
        ToolsRow,
        Scoped,
    ));
    let opps = state::opponents();
    let opp = &opps[s.round.min(opps.len() - 1)];
    commands
        .spawn((Node { position_type: PositionType::Absolute, bottom: Val::Px(16.0), left: Val::Px(0.0), right: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, column_gap: Val::Px(14.0), ..default() }, Pickable::IGNORE, Scoped))
        .with_children(|c| {
            ui::button(c, &fonts, "test", "Test drive", BtnStyle::Teal, 22.0);
            let f = ui::button(c, &fonts, "fight", &format!("Fight {}!", opp.design.name), BtnStyle::Primary, 26.0);
            c.commands().entity(f).insert(FightLabel);
        });
    commands.spawn((
        Node { position_type: PositionType::Absolute, padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
        BackgroundColor(Color::srgba(0.1, 0.09, 0.13, 0.88)),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(50),
        Tip,
        Scoped,
    )).with_children(|t| {
        t.spawn((ui::text(&fonts, "", 15.0, ui::CREAM), TextLayout::default().with_no_wrap(), Pickable::IGNORE));
    });

    commands.insert_resource(Ws {
        sel: None,
        sel_uid: None,
        roll: 0.0,
        tilt: 0.0,
        mirror: true,
        az: 0.7,
        elev: 0.5,
        dist: 5.2,
        down: None,
        moved: 0.0,
        last: Vec2::ZERO,
        hovered: None,
        plan: None,
        undo: vec![],
        bot_dirty: true,
        ui_dirty: true,
        ghost_key: None,
        ghosts,
        ghost_mat,
        bot: None,
        bot_y: BENCH_Y,
    });
}

// ------------------------------------------------------------------ systems
fn camera(
    mut ws: ResMut<Ws>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    over_ui: Res<PointerOverUi>,
    win: Query<&Window, With<PrimaryWindow>>,
    mut cam: Query<&mut Transform, With<WsCam>>,
) {
    let dt = time.delta_secs();
    if keys.pressed(KeyCode::KeyQ) {
        ws.az += dt * 1.8;
    }
    if keys.pressed(KeyCode::KeyE) {
        ws.az -= dt * 1.8;
    }
    let cursor = win.single().ok().and_then(|w| w.cursor_position());
    if let Some(c) = cursor {
        for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            if mouse.just_pressed(b) && !over_ui.0 {
                ws.down = Some(b);
                ws.moved = 0.0;
                ws.last = c;
            }
        }
        if ws.down.is_some_and(|b| mouse.pressed(b)) {
            let d = c - ws.last;
            ws.moved += d.x.abs() + d.y.abs();
            if ws.moved > 6.0 {
                ws.az -= d.x * 0.008;
                ws.elev = (ws.elev + d.y * 0.006).clamp(0.05, 1.4);
            }
            ws.last = c;
        }
    }
    if !over_ui.0 && scroll.delta.y != 0.0 {
        let k = if scroll.unit == bevy::input::mouse::MouseScrollUnit::Line { 0.4 } else { 0.004 };
        ws.dist = (ws.dist - scroll.delta.y * k).clamp(2.8, 10.0);
    }
    if let Ok(mut tf) = cam.single_mut() {
        let target = Vec3::new(0.0, BENCH_Y + 0.4, 0.0);
        let p = target + Vec3::new(ws.az.sin() * ws.elev.cos(), ws.elev.sin(), ws.az.cos() * ws.elev.cos()) * ws.dist;
        *tf = Transform::from_translation(p).looking_at(target, Vec3::Y);
    }
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut ws: ResMut<Ws>, mut s: ResMut<GameState>) {
    for (k, t) in [(KeyCode::KeyM, "m"), (KeyCode::KeyR, "r"), (KeyCode::KeyT, "t"), (KeyCode::KeyZ, "z")] {
        if keys.just_pressed(k) {
            ws.tool(t, &mut s);
        }
    }
    if keys.just_pressed(KeyCode::Escape) && ws.sel.is_some() {
        ws.sel = None;
        ws.ui_dirty = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn clicks(
    mut ev: MessageReader<Clicked>,
    mut ws: ResMut<Ws>,
    mut s: ResMut<GameState>,
    mut toast: MessageWriter<Toast>,
    mut next: ResMut<NextState<Screen>>,
    fight_btn: Query<&Children, With<FightLabel>>,
    mut texts: Query<&mut Text>,
) {
    for Clicked(id) in ev.read() {
        let (head, arg) = id.split_once(':').unwrap_or((id.as_str(), ""));
        let n: usize = arg.parse().unwrap_or(0);
        match head {
            "test" | "fight" => {
                if validate(&s, &mut toast) {
                    s.testing = head == "test";
                    next.set(Screen::Fight);
                }
            }
            "tool" => ws.tool(arg, &mut s),
            "remove" => {
                ws.sel = if ws.sel == Some(Sel::Remove) { None } else { Some(Sel::Remove) };
                ws.ui_dirty = true;
            }
            "kind" => {
                let k = KIND_ORDER[n];
                ws.sel = if ws.sel == Some(Sel::Kind(k)) { None } else { Some(Sel::Kind(k)) };
                ws.sel_uid = None;
                ws.ui_dirty = true;
            }
            "uid" => {
                if let Some(it) = s.inv.iter().find(|x| x.uid == n as u32) {
                    ws.sel = Some(Sel::Kind(it.kind));
                    ws.sel_uid = Some(it.uid);
                    ws.ui_dirty = true;
                }
            }
            "fixit" => {
                let Some(Sel::Kind(k)) = ws.sel else { continue };
                let Some(uid) = ws.pick(&s, k).map(|x| x.uid) else { continue };
                ws.snapshot(&s);
                let st = &mut *s;
                let it = st.inv.iter_mut().find(|x| x.uid == uid).unwrap();
                match fix(&mut st.plate, &mut st.bolts, k, &mut it.dmg) {
                    None => {
                        ws.undo.pop();
                        toast.write(Toast("Not enough plate or bolts. Time for the scrapheap...".into(), ui::RED));
                    }
                    Some(full) => {
                        crate::audio::place();
                        crate::audio::ding(1.0);
                        let name = k.def().name;
                        toast.write(Toast(if full { format!("{name} good as new") } else { format!("Patched the {name} up a bit") }, ui::GREEN));
                    }
                }
                ws.ui_dirty = true;
            }
            "repair" => {
                ws.snapshot(&s);
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
                    crate::audio::place();
                    crate::audio::ding(1.0);
                    let more = if partial > 0 { format!(", patched {partial} more") } else { String::new() };
                    toast.write(Toast(format!("Repaired {fixed} part{}{more}", if fixed == 1 { "" } else { "s" }), ui::GREEN));
                } else {
                    ws.undo.pop();
                    toast.write(Toast("Not enough plate or bolts. Time for the scrapheap...".into(), ui::RED));
                }
                ws.dirty();
            }
            "fab" => {
                let (k, c) = FAB_COST[n];
                if s.plate >= c {
                    ws.snapshot(&s);
                    s.plate -= c;
                    s.add_item(k, 0.0);
                    crate::audio::place();
                    toast.write(Toast(format!("Welded a {}", k.def().name), ui::CREAM));
                    ws.ui_dirty = true;
                }
            }
            "cheat" => {
                // testing shortcut: 10 fresh of every part, plus plate and bolts
                ws.snapshot(&s);
                for k in KIND_ORDER {
                    for _ in 0..10 {
                        s.add_item(k, 0.0);
                    }
                }
                s.plate += 30;
                s.bolts += 30;
                crate::audio::ding(1.0);
                toast.write(Toast("Test stash: +10 of every part, +30 plate and bolts".into(), ui::GREEN));
                ws.ui_dirty = true;
            }
            "opp" => {
                // dev: jump straight to any opponent
                s.round = n;
                let name = state::opponents()[n].design.name.clone();
                for ch in fight_btn.iter() {
                    for c in ch.iter() {
                        if let Ok(mut t) = texts.get_mut(c) {
                            t.0 = format!("Fight {name}!");
                        }
                    }
                }
                crate::audio::ding(1.0);
                ws.ui_dirty = true;
            }
            "paint" => {
                s.design.paint = Color::Srgba(Srgba::hex(PAINTS[n]).unwrap());
                ws.ghost_key = None;
                ws.dirty();
            }
            _ => {}
        }
    }
}

/// Ray-pick the part under the cursor (against each part's bounds box, which
/// also tells us which face we're on), build the placement plan, and handle
/// click / right-click.
#[allow(clippy::too_many_arguments)]
fn pick(
    mut ws: ResMut<Ws>,
    mut s: ResMut<GameState>,
    mouse: Res<ButtonInput<MouseButton>>,
    over_ui: Res<PointerOverUi>,
    win: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<WsCam>>,
    mut toast: MessageWriter<Toast>,
    mut gizmos: Gizmos,
) {
    let ws = &mut *ws;
    ws.hovered = None;
    ws.plan = None;
    // once a press turns into an orbit drag stop picking, so a click keeps its target
    let orbiting = ws.down.is_some_and(|b| mouse.pressed(b)) && ws.moved > 6.0;
    let ray = (|| {
        let c = win.single().ok()?.cursor_position()?;
        let (camera, gt) = cam.single().ok()?;
        camera.viewport_to_world(gt, c).ok()
    })();
    let root = Vec3::new(0.0, ws.bot_y, 0.0);
    let mut face_hit = None;
    if let (Some(ray), false, false) = (ray, over_ui.0, orbiting) {
        let mut best = f32::INFINITY;
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
    }
    let kind = match ws.sel {
        Some(Sel::Kind(k)) => Some(k),
        _ => None,
    };
    if let Some((id, face, u, v)) = face_hit {
        ws.hovered = Some(id);
        if let Some(k) = kind {
            let par = s.design.by_id(id).unwrap();
            ws.plan = Some(ws.make_plan(&s, k, par, face, u, v));
        }
    }
    // hover outline on the part you'd pick up / unbolt
    if let (Some(id), None) = (ws.hovered, kind) {
        if let Some(p) = s.design.by_id(id) {
            let b = p.kind.def().bounds;
            let col = if ws.sel == Some(Sel::Remove) { Color::srgb_u8(0xff, 0x9a, 0x4a) } else { ui::CREAM };
            gizmos.cube(Transform { translation: root + p.pos + p.rot * b.c, rotation: p.rot, scale: b.h * 2.0 + Vec3::splat(0.04) }, col);
        }
    }
    // clicks (only a press that didn't turn into a drag)
    for b in [MouseButton::Left, MouseButton::Right] {
        if !mouse.just_released(b) || ws.down != Some(b) {
            continue;
        }
        ws.down = None;
        if ws.moved > 6.0 {
            continue;
        }
        if b == MouseButton::Right {
            if ws.sel.is_some() {
                ws.sel = None;
                ws.ui_dirty = true;
            } else if let Some(h) = ws.hovered {
                ws.remove(&mut s, h, false, &mut toast);
            }
            continue;
        }
        match ws.sel {
            Some(Sel::Remove) => {
                if let Some(h) = ws.hovered {
                    ws.remove(&mut s, h, false, &mut toast);
                }
            }
            Some(Sel::Kind(_)) => ws.place(&mut s, &mut toast),
            None => {
                if let Some(h) = ws.hovered.filter(|h| s.design.by_id(*h).is_some_and(|p| p.kind != PartKind::Core)) {
                    ws.remove(&mut s, h, true, &mut toast);
                }
            }
        }
    }
}

fn rebuild_bot(mut commands: Commands, mut ws: ResMut<Ws>, s: Res<GameState>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, shared: Res<Mats>) {
    if !ws.bot_dirty {
        return;
    }
    ws.bot_dirty = false;
    ws.ui_dirty = true;
    if let Some(e) = ws.bot.take() {
        commands.entity(e).despawn();
    }
    let e = spawn_preview(&mut commands, &mut meshes, &mut mats, &shared, &s.design, true, Transform::default(), Scoped);
    let ride = -crate::parts::ground_level(&s.design);
    ws.bot_y = BENCH_Y + 0.06 + ride;
    commands.entity(e).insert(Transform::from_xyz(0.0, ws.bot_y, 0.0));
    ws.bot = Some(e);
}

fn ghosts(mut ws: ResMut<Ws>, s: Res<GameState>, time: Res<Time>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut q: Query<(&mut Mesh3d, &mut Transform, &mut Visibility)>) {
    let ws = &mut *ws;
    let root = Vec3::new(0.0, ws.bot_y, 0.0);
    let plan = ws.plan.as_ref();
    // rebuild the ghost mesh when the kind or its orientation changes (legs pose by orientation)
    let key = plan.map(|p| (p.kind, p.a.rot.to_array().map(|x| (x * 100.0) as i32)));
    if key != ws.ghost_key {
        ws.ghost_key = key;
        if let Some(p) = plan {
            let paint = models::col_of(s.design.paint);
            for (i, x) in [Some(p.a), p.b.map(|b| b.0)].into_iter().enumerate() {
                let Some(x) = x else { continue };
                let pl = Placed { id: 0, kind: p.kind, parent: None, pos: x.pos, rot: x.rot, dmg: 0.0, mirror: None };
                let (g, _) = part_geo(&pl, paint);
                if let Ok((mut m, _, _)) = q.get_mut(ws.ghosts[i]) {
                    m.0 = meshes.add(g.to_mesh());
                }
            }
        }
    }
    let xs = [plan.map(|p| p.a), plan.and_then(|p| p.b.map(|b| b.0))];
    for (i, x) in xs.into_iter().enumerate() {
        if let Ok((_, mut tf, mut vis)) = q.get_mut(ws.ghosts[i]) {
            *vis = if x.is_some() { Visibility::Visible } else { Visibility::Hidden };
            if let Some(x) = x {
                *tf = Transform { translation: root + x.pos, rotation: x.rot, ..default() };
            }
        }
    }
    if let Some(mut m) = mats.get_mut(&ws.ghost_mat) {
        let a = 0.4 + (time.elapsed_secs() * 8.0).sin() * 0.1;
        m.base_color = if plan.is_some_and(|p| p.why.is_some()) { Color::srgba(1.0, 0.35, 0.29, a) } else { Color::srgba(0.61, 1.0, 0.48, a) };
    }
}

fn tip(ws: Res<Ws>, s: Res<GameState>, win: Query<&Window, With<PrimaryWindow>>, mut q: Query<(&mut Node, &mut Visibility, &Children), With<Tip>>, mut texts: Query<&mut Text>) {
    let Ok((mut node, mut vis, ch)) = q.single_mut() else { return };
    let kind = match ws.sel {
        Some(Sel::Kind(k)) => Some(k),
        _ => None,
    };
    let msg = if let (Some(k), Some(p)) = (kind, &ws.plan) {
        match &p.why {
            Some(w) => format!("x  {w}"),
            None => {
                let pct = ws.pick(&s, k).map_or(100, |it| ((1.0 - it.dmg) * 100.0).round() as i32);
                format!("{}{}{}", k.def().name, if pct < 100 { format!(" ({pct}%)") } else { String::new() }, if p.b.is_some() { "  x2 mirrored" } else { "" })
            }
        }
    } else if let Some(k) = kind {
        format!("Point at your bot to place a {}", k.def().name)
    } else if let Some(p) = ws.hovered.and_then(|h| s.design.by_id(h)) {
        let hp = ((1.0 - p.dmg) * 100.0).round();
        let (res, n) = state::repair_cost(p.kind, p.dmg);
        let fixs = if n > 0 { format!(" · fix: {n} {}", if res == Stock::Plate { "plate" } else { "bolts" }) } else { String::new() };
        let hint = if p.kind != PartKind::Core { if ws.sel == Some(Sel::Remove) { " · click to unbolt" } else { " · click to move · right-click to unbolt" } } else { "" };
        format!("{} · {hp}% hp{fixs}{hint}", p.kind.def().name)
    } else {
        String::new()
    };
    let cursor = win.single().ok().and_then(|w| w.cursor_position());
    *vis = if msg.is_empty() || cursor.is_none() { Visibility::Hidden } else { Visibility::Visible };
    if let Some(c) = cursor {
        node.left = Val::Px(c.x + 16.0);
        node.top = Val::Px(c.y - 34.0);
    }
    for c in ch.iter() {
        if let Ok(mut t) = texts.get_mut(c) {
            if t.0 != msg {
                t.0 = msg.clone();
            }
        }
    }
}

// ------------------------------------------------------------------ UI
fn section(c: &mut ChildSpawnerCommands, fonts: &Fonts, s: &str) {
    c.spawn((ui::title_text(fonts, s.to_uppercase(), 15.0, ui::TEAL), Pickable::IGNORE));
}

fn btn(c: &mut ChildSpawnerCommands, fonts: &Fonts, id: &str, label: &str, style: BtnStyle, size: f32, disabled: bool) -> Entity {
    let e = ui::button(c, fonts, id, label, style, size);
    if disabled {
        c.commands().entity(e).insert(Btn { id: id.into(), style, disabled: true });
    }
    e
}

/// A clickable node with custom content.
fn clickable(c: &mut ChildSpawnerCommands, id: String, style: BtnStyle, node: Node, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    c.spawn((Btn { id, style, disabled: false }, Node { border_radius: BorderRadius::all(Val::Px(7.0)), align_items: AlignItems::Center, ..node }, BackgroundColor(style.colours().0))).with_children(f);
}

fn wear_col(pct: i32) -> Color {
    if pct >= 70 { ui::GREEN } else if pct >= 40 { ui::YELLOW } else { ui::RED }
}

#[allow(clippy::too_many_arguments)]
fn refresh_ui(
    mut commands: Commands,
    mut ws: ResMut<Ws>,
    s: Res<GameState>,
    fonts: Res<Fonts>,
    left: Query<Entity, With<LeftCol>>,
    right: Query<Entity, With<RightCol>>,
    tools: Query<Entity, With<ToolsRow>>,
) {
    if !ws.ui_dirty {
        return;
    }
    ws.ui_dirty = false;
    let fonts = &*fonts;
    let st = compute_stats(&s.design);
    let sel_bg = ui::YELLOW;
    let row_bg = Color::srgba(1.0, 1.0, 1.0, 0.05);

    // ---- left: parts bin + description
    if let Ok(e) = left.single() {
        commands.entity(e).despawn_related::<Children>().with_children(|col| {
            col.spawn(ui::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), flex_shrink: 1.0, overflow: Overflow::clip(), ..default() })).with_children(|p| {
                section(p, fonts, "Parts bin");
                let on = ws.sel == Some(Sel::Remove);
                clickable(p, "remove".into(), if on { BtnStyle::Custom(sel_bg) } else { BtnStyle::Custom(row_bg) }, Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), column_gap: Val::Px(8.0), ..default() }, |r| {
                    r.spawn((Node { width: Val::Px(14.0), height: Val::Px(14.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, BackgroundColor(Color::srgb_u8(0x5b, 0x56, 0x6a)), Pickable::IGNORE));
                    r.spawn((ui::text(fonts, "Unbolt tool", 15.0, if on { ui::INK } else { ui::CREAM }), Pickable::IGNORE));
                });
                for (ki, k) in KIND_ORDER.iter().enumerate() {
                    let d = k.def();
                    let mut items: Vec<&InvItem> = s.inv.iter().filter(|x| x.kind == *k).collect();
                    items.sort_by(|a, b| a.dmg.total_cmp(&b.dmg));
                    let n = items.len();
                    let on = ws.sel == Some(Sel::Kind(*k));
                    let fg = if on { ui::INK } else if n == 0 { Color::srgba(1.0, 0.95, 0.86, 0.35) } else { ui::CREAM };
                    clickable(p, format!("kind:{ki}"), BtnStyle::Custom(if on { sel_bg } else { row_bg }), Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)), column_gap: Val::Px(8.0), ..default() }, |r| {
                        r.spawn((Node { width: Val::Px(14.0), height: Val::Px(14.0), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, BackgroundColor(d.colour.with_alpha(if n == 0 { 0.35 } else { 1.0 })), Pickable::IGNORE));
                        r.spawn((ui::text(fonts, d.name, 15.0, fg), Node { flex_grow: 1.0, ..default() }, Pickable::IGNORE));
                        r.spawn((ui::title_text(fonts, format!("{n}"), 15.0, fg), Pickable::IGNORE));
                    });
                    // the actual roster: shown when it's worth choosing (some worn) or selected
                    if n > 0 && (on || items.iter().any(|x| x.dmg > 0.01)) {
                        let cur = if on { ws.pick(&s, *k).map(|x| x.uid) } else { None };
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
                                clickable(chips, format!("uid:{uid}"), BtnStyle::Custom(if sel { Color::srgb_u8(0x5a, 0x55, 0x66) } else { Color::srgba(0.0, 0.0, 0.0, 0.25) }), Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)), column_gap: Val::Px(5.0), border: UiRect::all(Val::Px(if sel { 2.0 } else { 0.0 })), ..default() }, |c| {
                                    c.spawn((Node { width: Val::Px(22.0), height: Val::Px(5.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12)), Pickable::IGNORE)).with_children(|bar| {
                                        bar.spawn((Node { width: Val::Percent(pct as f32), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, BackgroundColor(wear_col(pct)), Pickable::IGNORE));
                                    });
                                    c.spawn((ui::text(fonts, label, 13.0, ui::CREAM), Pickable::IGNORE));
                                });
                            }
                        });
                    }
                }
            });
            col.spawn(ui::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), flex_shrink: 0.0, ..default() })).with_children(|p| {
                match ws.sel {
                    Some(Sel::Kind(k)) => {
                        let d = k.def();
                        p.spawn(ui::title_text(fonts, d.name, 18.0, ui::YELLOW));
                        p.spawn(ui::text(fonts, format!("{}kg · {}{} power", d.mass, if d.power > 0.0 { "+" } else { "" }, d.power), 14.0, Color::srgb_u8(0xd9, 0xcc, 0xb4)));
                        p.spawn(ui::text(fonts, d.desc, 14.0, ui::CREAM));
                        if let Some(it) = ws.pick(&s, k).filter(|it| it.dmg > 0.01) {
                            let (res, n) = state::repair_cost(k, it.dmg);
                            let have = if res == Stock::Plate { s.plate } else { s.bolts };
                            p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                                let pct = ((1.0 - it.dmg) * 100.0).round() as i32;
                                r.spawn(ui::text(fonts, format!("Condition {pct}%"), 14.0, wear_col(pct)));
                                btn(r, fonts, "fixit", &format!("Fix: {n} {}", if res == Stock::Plate { "plate" } else { "bolts" }), BtnStyle::Secondary, 14.0, have <= 0);
                            });
                        }
                    }
                    Some(Sel::Remove) => {
                        p.spawn(ui::text(fonts, "Click any part to unbolt it (and everything attached to it) back into the bin.", 14.0, ui::CREAM));
                    }
                    None => {
                        p.spawn(ui::text(fonts, "Pick a part, then click any face of your bot to bolt it on. Click a fitted part to pick it up and move it. Drag to orbit, scroll to zoom.", 14.0, ui::CREAM));
                    }
                }
            });
        });
    }

    // ---- right: opponent, stats, stash, paint
    if let Ok(e) = right.single() {
        let opps = state::opponents();
        let opp = &opps[s.round.min(opps.len() - 1)];
        let damaged: Vec<&Placed> = s.design.parts.iter().filter(|c| c.dmg > 0.01).collect();
        let (mut plate_need, mut bolt_need) = (0, 0);
        for c in &damaged {
            let (r, n) = state::repair_cost(c.kind, c.dmg);
            if r == Stock::Plate { plate_need += n } else { bolt_need += n }
        }
        commands.entity(e).despawn_related::<Children>().with_children(|col| {
            col.spawn(ui::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() })).with_children(|p| {
                section(p, fonts, &format!("Round {} of {} · vs {}", s.round + 1, opps.len(), opp.design.name));
                p.spawn(Node { column_gap: Val::Px(5.0), align_items: AlignItems::Center, ..default() }).with_children(|l| {
                    // dev: click a round to jump straight to that opponent
                    for i in 0..opps.len() {
                        let style = if i < s.round { BtnStyle::Custom(Color::srgba(0.486, 0.812, 0.353, 0.3)) } else if i == s.round { BtnStyle::Custom(ui::YELLOW) } else { BtnStyle::Custom(Color::srgba(1.0, 1.0, 1.0, 0.08)) };
                        clickable(l, format!("opp:{i}"), style, Node { width: Val::Px(28.0), height: Val::Px(24.0), justify_content: JustifyContent::Center, ..default() }, |c| {
                            c.spawn((ui::title_text(fonts, format!("{}", i + 1), 15.0, if i == s.round { ui::INK } else { ui::CREAM }), Pickable::IGNORE));
                        });
                    }
                    l.spawn(ui::text(fonts, format!("  lives {}", s.lives), 14.0, ui::RED));
                });
                p.spawn(ui::text(fonts, opp.blurb, 14.0, ui::CREAM));
            });
            col.spawn(ui::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(5.0), ..default() })).with_children(|p| {
                section(p, fonts, &s.design.name);
                for (name, v, c) in [("Speed", st.speed, ui::YELLOW), ("Armour", st.armour, ui::TEAL), ("Bite", st.bite, ui::RED), ("Balance", st.balance, Color::srgb_u8(0xa8, 0x8c, 0xf0))] {
                    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|r| {
                        r.spawn(ui::text(fonts, name, 15.0, ui::CREAM));
                        ui::bars(r, v, c);
                    });
                }
                let pw = if st.power_ratio < 1.0 { format!(" ({}%)", (st.power_ratio * 100.0).round()) } else { String::new() };
                p.spawn(ui::text(fonts, format!("{:.1} kg · {} parts · power {}/{}{pw}", st.mass, s.design.parts.len(), st.supply, st.demand), 14.0, Color::srgb_u8(0xd9, 0xcc, 0xb4)));
                for w in &st.warnings {
                    p.spawn(ui::text(fonts, format!("! {w}"), 14.0, Color::srgb_u8(0xff, 0x9a, 0x6a)));
                }
            });
            col.spawn(ui::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), ..default() })).with_children(|p| {
                section(p, fonts, "Stash");
                p.spawn(ui::title_text(fonts, format!("Bolts {}    Plate {}", s.bolts, s.plate), 17.0, ui::CREAM));
                if damaged.is_empty() {
                    p.spawn(ui::text(fonts, "Everything is in one piece", 14.0, ui::GREEN));
                } else {
                    p.spawn(ui::text(fonts, format!("{} damaged part{} · needs {plate_need} plate, {bolt_need} bolts", damaged.len(), if damaged.len() > 1 { "s" } else { "" }), 14.0, ui::CREAM));
                }
                p.spawn(Node { column_gap: Val::Px(5.0), row_gap: Val::Px(5.0), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|r| {
                    btn(r, fonts, "repair", "Repair", BtnStyle::Secondary, 15.0, damaged.is_empty());
                    for (i, (k, c)) in FAB_COST.iter().enumerate() {
                        btn(r, fonts, &format!("fab:{i}"), &format!("+{} {c}p", k.def().name), BtnStyle::Secondary, 12.0, s.plate < *c);
                    }
                });
                btn(p, fonts, "cheat", "Test: +10 of every part", BtnStyle::Secondary, 13.0, false);
            });
            col.spawn(ui::panel(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() })).with_children(|p| {
                section(p, fonts, "Paint");
                p.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                    for (i, h) in PAINTS.iter().enumerate() {
                        let c = Color::Srgba(Srgba::hex(h).unwrap());
                        let on = s.design.paint.to_srgba().to_hex() == c.to_srgba().to_hex();
                        clickable(r, format!("paint:{i}"), BtnStyle::Custom(c), Node { width: Val::Px(26.0), height: Val::Px(26.0), border: UiRect::all(Val::Px(if on { 3.0 } else { 0.0 })), ..default() }, |_| {});
                    }
                });
            });
        });
    }

    // ---- tools
    if let Ok(e) = tools.single() {
        let deg = (ws.roll.to_degrees()).round();
        let tdeg = (ws.tilt.to_degrees()).round();
        let mirror = ws.mirror;
        let can_undo = !ws.undo.is_empty();
        commands.entity(e).despawn_related::<Children>().with_children(|r| {
            btn(r, fonts, "tool:m", &format!("Mirror {} [M]", if mirror { "on" } else { "off" }), if mirror { BtnStyle::Teal } else { BtnStyle::Secondary }, 15.0, false);
            btn(r, fonts, "tool:r", &format!("Roll {deg}° [R]"), BtnStyle::Secondary, 15.0, false);
            btn(r, fonts, "tool:t", &format!("Tilt {tdeg}° [T]"), BtnStyle::Secondary, 15.0, false);
            btn(r, fonts, "tool:z", "Undo [Z]", BtnStyle::Secondary, 15.0, !can_undo);
        });
    }
}
