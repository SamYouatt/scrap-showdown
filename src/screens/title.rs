//! Title screen: three champion bots bobbing on a dirt plinth under a blue sky.
use super::Screen;
use crate::audio::{Sfx, Snd};
use crate::models::{self, Geo, Mats};
use crate::preview::{spawn_preview, PreviewRoot};
use crate::scene::{self, SunOpts};
use crate::state::{self, Run};
use crate::theme::prelude::*;
use bevy::prelude::*;
use std::f32::consts::PI;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Title), spawn_title);
    app.add_systems(Update, (bob_bots, sway_camera).run_if(in_state(Screen::Title)));
}

#[derive(Component)]
struct TitleBot(usize);
#[derive(Component)]
struct TitleCam;

fn bot_yaw(i: f32) -> f32 {
    PI + (i - 1.0) * -0.5
}

fn spawn_title(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, shared: Res<Mats>, fonts: Res<Fonts>) {
    let scope = DespawnOnExit(Screen::Title);
    models::reseed(3);
    scene::sun_lights(&mut commands, SunOpts { size: 6.0, dir: Vec3::new(4.0, 8.0, -6.0), ..default() }, scope.clone());
    scene::sky_and_clouds(&mut commands, &mut meshes, &mut mats, Color::srgb_u8(0x3f, 0x93, 0xe6), Color::srgb_u8(0xb8, 0xdc, 0xf6), 10, 30.0, scope.clone());
    let ground: Geo = models::cyl(6.0, 6.0, 0.4, 10, models::hexc("#c79a62"), 0.0);
    commands.spawn((Name::new("Plinth"), Mesh3d(meshes.add(ground.to_mesh())), MeshMaterial3d(shared.vc.clone()), Transform::from_xyz(0.0, -0.2, 0.0), scope.clone()));
    let opps = state::opponents();
    for (i, idx) in [4usize, 1, 3].into_iter().enumerate() {
        let x = (i as f32 - 1.0) * 2.6;
        let z = if i == 1 { -0.6 } else { 0.4 };
        let tf = Transform::from_xyz(x, 0.0, z).with_rotation(Quat::from_rotation_y(bot_yaw(i as f32)));
        let e = spawn_preview(&mut commands, &mut meshes, &mut mats, &shared, &opps[idx].design, false, tf, scope.clone());
        commands.entity(e).insert(TitleBot(i));
    }
    commands.spawn((scene::camera_bundle(36.0, Transform::from_xyz(0.0, 3.4, -9.5).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y)), TitleCam, scope.clone()));

    // logo top, blurb + start button bottom
    let root = Node { height: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, padding: UiRect::vertical(Val::Vh(5.0)), ..default() };
    commands.spawn((widget::ui_root("Title UI", root), scope)).with_children(|p| {
        p.spawn((
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
            Pickable::IGNORE,
            children![
                (widget::headline(&fonts, "SCRAPHEAP", 96.0, YELLOW, 6.0, 0.45), Pickable::IGNORE),
                (widget::headline(&fonts, "RUMBLE", 64.0, RED, 5.0, 0.45), Pickable::IGNORE),
            ],
        ));
        p.spawn(widget::panel(Node { width: Val::Px(640.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(12.0), ..default() })).with_children(|c| {
            c.spawn((
                widget::text(&fonts, "Build a battle bot from junk. Fight five opponents.\nBetween fights, raid the scrapheap for parts — the better you fight, the longer you get to dig.", 17.0, CREAM),
                TextLayout::default().with_justify(Justify::Center),
            ));
            c.spawn(widget::button(&fonts, "Start a run", BtnStyle::Primary, 30.0)).observe(start_run);
        });
    });
}

fn start_run(_: On<Activate>, mut run: ResMut<Run>, mut next: ResMut<NextState<Screen>>, mut sfx: MessageWriter<Sfx>) {
    sfx.write(Snd::Ding.pitched(1.0));
    *run = Run::new_game();
    next.set(Screen::Workshop);
}

fn bob_bots(time: Res<Time>, mut bots: Query<(&TitleBot, &PreviewRoot, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (b, r, mut tf) in &mut bots {
        let i = b.0 as f32;
        tf.translation.y = r.ride + (t * 3.0 + i * 1.3).sin().abs() * 0.12;
        tf.rotation = Quat::from_rotation_y(bot_yaw(i)) * Quat::from_rotation_z((t * 3.0 + i).sin() * 0.05);
    }
}

fn sway_camera(time: Res<Time>, mut cam: Single<&mut Transform, With<TitleCam>>) {
    cam.translation.x = (time.elapsed_secs() * 0.2).sin() * 1.5;
    cam.look_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y);
}
