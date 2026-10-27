//! Native screenshot for checking art: `SCRAP_SHOT=out.png` renders every
//! camera offscreen (so it works with the window hidden or the display asleep)
//! and saves a frame `SCRAP_SHOT_DELAY` seconds (default 6) after the first
//! screen loads, then quits. `SCRAP_SHOT_SIZE=1600x900` sets the resolution;
//! `SCRAP_CAM=x,y,z,tx,ty,tz` overrides every camera to look from x,y,z at tx,ty,tz.
use crate::params::param;
use crate::screens::Screen;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

pub(super) fn plugin(app: &mut App) {
    if param("shot").is_some() {
        app.add_systems(Startup, make_target);
        app.add_systems(Update, (retarget, shoot.run_if(not(in_state(Screen::Loading)))));
        app.add_systems(PostUpdate, override_cam.before(bevy::transform::TransformSystems::Propagate));
    }
}

#[derive(Resource)]
struct Target(Handle<Image>);

fn make_target(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let (w, h) = param("shot_size").and_then(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))).unwrap_or((1600, 900));
    let mut img = Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None);
    img.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    commands.insert_resource(Target(images.add(img)));
}

fn retarget(mut commands: Commands, target: Res<Target>, cams: Query<Entity, Added<Camera>>) {
    for e in &cams {
        commands.entity(e).insert(RenderTarget::Image(target.0.clone().into()));
    }
}

fn shoot(mut commands: Commands, target: Res<Target>, time: Res<Time<Real>>, mut t0: Local<Option<f32>>, mut state: Local<u8>, mut exit: MessageWriter<AppExit>) {
    let now = time.elapsed_secs();
    let start = *t0.get_or_insert(now);
    let delay: f32 = param("shot_delay").and_then(|v| v.parse().ok()).unwrap_or(6.0);
    match *state {
        0 if now - start > delay => {
            commands.spawn(Screenshot::image(target.0.clone())).observe(save_to_disk(param("shot").unwrap()));
            *state = 1;
        }
        1.. if now - start > delay + 1.5 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn override_cam(mut cams: Query<&mut Transform, With<Camera>>) {
    let Some(v) = param("cam").map(|s| s.split(',').filter_map(|x| x.parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 6) else { return };
    for mut tf in &mut cams {
        *tf = Transform::from_xyz(v[0], v[1], v[2]).looking_at(Vec3::new(v[3], v[4], v[5]), Vec3::Y);
    }
}
