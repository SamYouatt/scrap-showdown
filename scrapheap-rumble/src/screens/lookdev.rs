//! Look-dev turntable (`?scene=lookdev`): every Blender-made part around a test
//! bot, under the game's lighting.
//! Drag to orbit, scroll to zoom, Space toggles the turntable.
//! `&yaw=&pitch=&dist=` (degrees, metres) fix the starting view for screenshots.
use super::Screen;
use crate::models::{self, Mats};
use crate::params::param;
use crate::art::Art;
use crate::parts::{ground_level, Builder, Design, Face, PartKind, Placed, KIND_ORDER, LEG_LEN};
use crate::preview::{part_look, spawn_preview, spawn_static_part};
use crate::robot::EyeMats;
use crate::scene::{self, SunOpts};
use crate::theme::prelude::*;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use std::f32::consts::PI;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Lookdev), spawn_lookdev);
    app.add_systems(Update, (orbit, turntable, cycle_tonemapping).run_if(in_state(Screen::Lookdev)));
}

#[derive(Resource)]
struct View {
    yaw: f32,
    pitch: f32,
    dist: f32,
    spin: bool,
}

#[derive(Component)]
struct Cam;
#[derive(Component)]
struct Turntable;
#[derive(Component)]
struct TmLabel;

const TONEMAPPERS: [(&str, Tonemapping); 5] = [
    ("aces", Tonemapping::AcesFitted),
    ("tony", Tonemapping::TonyMcMapface),
    ("agx", Tonemapping::AgX),
    ("neutral", Tonemapping::KhronosPbrNeutral),
    ("filmic", Tonemapping::BlenderFilmic),
];

fn p(key: &str, default: f32) -> f32 {
    param(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Core with a wheel bolted to each side, the way the workshop places parts.
fn test_bot() -> Design {
    let mut b = Builder::new("Current", "#d23a2c");
    b.add_m(0, Face::PX, PartKind::Wheel);
    b.add(0, Face::PY, PartKind::Spinner);
    b.d
}

fn spawn_lookdev(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, shared: Res<Mats>, fonts: Res<Fonts>, art: Res<Art>, eyes: Res<EyeMats>) {
    let scope = DespawnOnExit(Screen::Lookdev);
    models::reseed(5);
    commands.insert_resource(View { yaw: p("yaw", 25.0).to_radians(), pitch: p("pitch", 14.0).to_radians(), dist: p("dist", 6.0), spin: param("spin").is_some() });

    scene::sun_lights(&mut commands, SunOpts { size: 4.0, dir: Vec3::new(-5.0, 6.0, 4.0), sun: Color::srgb_u8(0xff, 0xc8, 0x82), intensity: p("sun", 1.0), ..default() }, scope.clone());
    scene::sky_and_clouds(&mut commands, &mut meshes, &mut mats, Color::srgb_u8(0x3a, 0x8c, 0xe6), Color::srgb_u8(0xb4, 0xdc, 0xf8), 10, 30.0, scope.clone());
    let plinth = models::cyl(3.2, 3.2, 0.3, 24, models::hexc("#c79a62"), 0.0);
    commands.spawn((Name::new("Plinth"), Mesh3d(meshes.add(plinth.to_mesh())), MeshMaterial3d(shared.vc.clone()), Transform::from_xyz(0.0, -0.15, 0.0), scope.clone()));
    let tm = TONEMAPPERS.iter().find(|t| Some(t.0) == param("tm").as_deref()).unwrap_or(&TONEMAPPERS[0]);
    commands.spawn((scene::camera_bundle(34.0, Transform::default()), Cam, scope.clone())).insert(tm.1);

    let table = commands.spawn((Name::new("Turntable"), Transform::default(), Visibility::default(), Turntable, scope.clone())).id();
    // centre: a test bot; around it, every part on its own
    let d = test_bot();
    let bot = spawn_preview(&mut commands, &mut meshes, &art, &eyes, &d, false, Transform::from_xyz(0.0, -ground_level(&d), 0.0), scope.clone());
    commands.entity(table).add_child(bot);
    let n = KIND_ORDER.len();
    for (i, &kind) in KIND_ORDER.iter().enumerate() {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let pl = Placed { id: 0, kind, parent: None, pos: Vec3::ZERO, rot: Quat::IDENTITY, dmg: 0.0, mirror: None };
        // stand each part on the plinth facing outwards
        let lift = -kind.def().bounds.c.y + kind.def().bounds.h.y + if kind == PartKind::Leg { LEG_LEN } else { 0.0 };
        let tf = Transform::from_xyz(a.sin() * 2.2, lift, a.cos() * 2.2).with_rotation(Quat::from_rotation_y(a));
        let e = spawn_static_part(&mut commands, &mut meshes, &art, &pl, &part_look(d.paint, &eyes), tf, scope.clone());
        commands.entity(table).add_child(e);
    }

    if param("ui") != Some("0".into()) {
        let root = Node { width: Val::Percent(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::vertical(Val::Px(16.0)), ..default() };
        commands.spawn((widget::ui_root("Lookdev UI", root), scope)).with_children(|p| {
            p.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceAround, ..default() }).with_children(|r| {
                r.spawn(widget::text(&fonts, "Parts (Blender, baked textures)", 18.0, CREAM));
            });
            p.spawn((widget::text(&fonts, hint(tm.0), 14.0, CREAM), TmLabel));
        });
    }
}

fn orbit(mut v: ResMut<View>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, motion: Res<AccumulatedMouseMotion>, scroll: Res<AccumulatedMouseScroll>, mut cam: Single<&mut Transform, With<Cam>>) {
    if mouse.pressed(MouseButton::Left) || mouse.pressed(MouseButton::Right) {
        v.yaw -= motion.delta.x * 0.008;
        v.pitch = (v.pitch + motion.delta.y * 0.006).clamp(-0.1, 1.4);
    }
    if scroll.delta.y != 0.0 {
        let k = if scroll.unit == MouseScrollUnit::Line { 0.3 } else { 0.003 };
        v.dist = (v.dist - scroll.delta.y * k).clamp(1.2, 12.0);
    }
    if keys.just_pressed(KeyCode::Space) {
        v.spin = !v.spin;
    }
    let target = Vec3::new(0.0, 0.3, 0.0);
    let dir = Vec3::new(v.yaw.sin() * v.pitch.cos(), v.pitch.sin(), v.yaw.cos() * v.pitch.cos());
    **cam = Transform::from_translation(target + dir * v.dist).looking_at(target, Vec3::Y);
}

fn turntable(v: Res<View>, time: Res<Time>, mut t: Single<&mut Transform, With<Turntable>>) {
    if v.spin {
        t.rotate_y(time.delta_secs() * PI / 8.0);
    }
}

fn cycle_tonemapping(keys: Res<ButtonInput<KeyCode>>, mut cam: Single<&mut Tonemapping, With<Cam>>, mut label: Single<&mut Text, With<TmLabel>>) {
    if !keys.just_pressed(KeyCode::KeyT) {
        return;
    }
    let i = TONEMAPPERS.iter().position(|t| t.1 == **cam).map_or(0, |i| (i + 1) % TONEMAPPERS.len());
    **cam = TONEMAPPERS[i].1;
    label.0 = hint(TONEMAPPERS[i].0);
}

fn hint(tm: &str) -> String {
    format!("Drag to orbit · scroll to zoom · Space spins · T tonemapping ({tm})")
}
