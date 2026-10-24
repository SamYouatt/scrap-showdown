//! Title screen: three champion bots bobbing on a dirt plinth under a blue sky.
use crate::models::{self, Geo, Mats};
use crate::preview::{spawn_preview, PreviewRoot};
use crate::scene::{self, SunOpts};
use crate::state::{self, GameState};
use crate::ui::{self, BtnStyle, Clicked, Fonts};
use crate::{Scoped, Screen};
use bevy::prelude::*;

pub struct TitlePlugin;
impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Title), enter)
            .add_systems(Update, (animate, clicks).run_if(in_state(Screen::Title)));
    }
}

#[derive(Component)]
struct TitleBot(usize);
#[derive(Component)]
struct TitleCam;

fn enter(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, shared: Res<Mats>, fonts: Res<Fonts>) {
    models::reseed(3);
    scene::sun_lights(&mut commands, SunOpts { size: 6.0, dir: Vec3::new(4.0, 8.0, -6.0), ..default() }, Scoped);
    scene::sky_and_clouds(&mut commands, &mut meshes, &mut mats, Color::srgb_u8(0x4f, 0x9f, 0xe0), Color::srgb_u8(0xff, 0xe2, 0xb8), 10, 30.0, Scoped);
    let ground: Geo = models::cyl(6.0, 6.0, 0.4, 10, models::hexc("#c79a62"), 0.0);
    commands.spawn((Mesh3d(meshes.add(ground.to_mesh())), MeshMaterial3d(shared.vc.clone()), Transform::from_xyz(0.0, -0.2, 0.0), Scoped));
    let opps = state::opponents();
    for (i, idx) in [4usize, 1, 3].into_iter().enumerate() {
        let d = &opps[idx].design;
        let x = (i as f32 - 1.0) * 2.6;
        let z = if i == 1 { -0.6 } else { 0.4 };
        let rot = Quat::from_rotation_y(std::f32::consts::PI + (i as f32 - 1.0) * -0.5);
        let e = spawn_preview(&mut commands, &mut meshes, &mut mats, &shared, d, false, Transform::from_xyz(x, 0.0, z).with_rotation(rot), Scoped);
        commands.entity(e).insert(TitleBot(i));
    }
    commands.spawn((scene::camera_bundle(36.0, Transform::from_xyz(0.0, 3.4, -9.5).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y)), TitleCam, Scoped));

    // UI: logo top, blurb + start button bottom
    commands
        .spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, padding: UiRect::vertical(Val::Vh(5.0)), ..default() }, Pickable::IGNORE, Scoped))
        .with_children(|p| {
            p.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() }, Pickable::IGNORE)).with_children(|l| {
                l.spawn((ui::title_text(&fonts, "SCRAPHEAP", 96.0, ui::YELLOW), TextShadow { offset: Vec2::new(0.0, 6.0), color: Color::srgba(0.0, 0.0, 0.0, 0.45) }, Pickable::IGNORE));
                l.spawn((ui::title_text(&fonts, "RUMBLE", 64.0, ui::RED), TextShadow { offset: Vec2::new(0.0, 5.0), color: Color::srgba(0.0, 0.0, 0.0, 0.45) }, Pickable::IGNORE));
            });
            p.spawn(ui::panel(Node { width: Val::Px(640.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(12.0), ..default() })).with_children(|c| {
                c.spawn((
                    ui::text(&fonts, "Build a battle bot from junk. Fight five opponents.\nBetween fights, raid the scrapheap for parts — the better you fight, the longer you get to dig.", 17.0, ui::CREAM),
                    TextLayout::default().with_justify(Justify::Center),
                ));
                ui::button(c, &fonts, "start", "Start a run", BtnStyle::Primary, 30.0);
            });
        });
}

fn animate(time: Res<Time>, mut bots: Query<(&TitleBot, &PreviewRoot, &mut Transform), Without<TitleCam>>, mut cam: Query<&mut Transform, With<TitleCam>>) {
    let t = time.elapsed_secs();
    for (b, r, mut tf) in bots.iter_mut() {
        let i = b.0 as f32;
        tf.translation.y = r.ride + (t * 3.0 + i * 1.3).sin().abs() * 0.12;
        let yaw = std::f32::consts::PI + (i - 1.0) * -0.5;
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z((t * 3.0 + i).sin() * 0.05);
    }
    if let Ok(mut c) = cam.single_mut() {
        c.translation.x = (t * 0.2).sin() * 1.5;
        c.look_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y);
    }
}

fn clicks(mut ev: MessageReader<Clicked>, mut s: ResMut<GameState>, mut next: ResMut<NextState<Screen>>) {
    for Clicked(id) in ev.read() {
        if id == "start" {
            crate::audio::ding(1.0);
            *s = GameState::new_game();
            next.set(Screen::Workshop);
        }
    }
}
