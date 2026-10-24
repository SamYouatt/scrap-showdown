//! Shared scene dressing: golden-hour lights, gradient sky + clouds, and the
//! camera "look" (HDR, ACES, bloom, MSAA).
use crate::models::{self, Geo};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::camera::Hdr;
use std::f32::consts::TAU;

pub struct SunOpts {
    pub sky: Color,
    pub ground: Color,
    pub sun: Color,
    pub intensity: f32,
    pub dir: Vec3,
    /// half-size of the area that needs shadows
    pub size: f32,
}
impl Default for SunOpts {
    fn default() -> Self {
        SunOpts {
            sky: Color::srgb_u8(0xb4, 0xc4, 0xe4),
            ground: Color::srgb_u8(0xc0, 0x8a, 0x58),
            sun: Color::srgb_u8(0xff, 0xc9, 0x85),
            intensity: 1.0,
            dir: Vec3::new(-6.0, 12.0, -5.0),
            size: 14.0,
        }
    }
}

/// Golden hour: a warm low key light over a cool sky fill and a warm ground
/// bounce (a hemisphere light, approximated with ambient + an upward fill).
pub fn sun_lights(commands: &mut Commands, o: SunOpts, scope: impl Bundle + Clone) {
    commands.spawn((
        DirectionalLight { color: o.sun, illuminance: 9_000.0 * o.intensity, shadow_maps_enabled: true, ..default() },
        Transform::from_translation(o.dir).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: o.size * 3.5, first_cascade_far_bound: o.size * 3.5, ..default() }.build(),
        scope.clone(),
    ));
    // sky fill (WebGL2 allows only one directional light, so the warm ground
    // bounce is folded into the ambient colour)
    let fill = models::col_of(o.sky).lerp(models::col_of(o.ground), 0.3);
    commands.insert_resource(GlobalAmbientLight { color: models::to_color(fill), brightness: 1400.0 * o.intensity, ..default() });
    let _ = scope;
}

/// Inverted gradient sphere + low-poly clouds circling the scene.
pub fn sky_and_clouds(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    top: Color,
    bottom: Color,
    n: usize,
    radius: f32,
    scope: impl Bundle + Clone,
) {
    let (t, b) = (models::col_of(top), models::col_of(bottom));
    let mut sky = Geo::new();
    // a coarse sphere, coloured by height; drawn from the inside, unlit
    let (rings, segs) = (12, 16);
    let p = |i: usize, j: usize| {
        let th = i as f32 / rings as f32 * std::f32::consts::PI;
        let ph = j as f32 / segs as f32 * TAU;
        Vec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin())
    };
    let col = |v: Vec3| b.lerp(t, (v.y * 1.4 + 0.15).clamp(0.0, 1.0));
    for i in 0..rings {
        for j in 0..segs {
            let (a, bb, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
            for (x, y, z) in [(a, c, bb), (a, d, c)] {
                sky.pos.extend([x * 180.0, y * 180.0, z * 180.0]);
                let n = -(y - x).cross(z - x).normalize_or_zero();
                sky.nrm.extend([n, n, n]);
                sky.col.extend([col(x), col(y), col(z)]);
            }
        }
    }
    commands.spawn((
        Mesh3d(meshes.add(sky.to_mesh())),
        MeshMaterial3d(mats.add(StandardMaterial { unlit: true, cull_mode: None, fog_enabled: false, ..default() })),
        NoFrustumCulling,
        bevy::light::NotShadowCaster,
        scope.clone(),
    ));
    let cloud_mat = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(0.35, 0.35, 0.35), perceptual_roughness: 1.0, ..default() });
    for _ in 0..n {
        let c = models::cloud();
        let a = models::rand() * TAU;
        let pos = Vec3::new(a.cos() * radius, 14.0 + models::rand() * 10.0, a.sin() * radius);
        let s = 1.4 + models::rand() * 1.6;
        commands.spawn((
            Mesh3d(meshes.add(c.to_mesh())),
            MeshMaterial3d(cloud_mat.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(s)).looking_at(Vec3::new(0.0, pos.y, 0.0), Vec3::Y),
            bevy::light::NotShadowCaster,
            scope.clone(),
        ));
    }
}

/// The game camera with the painted-junk look.
pub fn camera_bundle(fov_deg: f32, transform: Transform) -> impl Bundle {
    (
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: fov_deg.to_radians(), near: 0.1, far: 400.0, ..default() }),
        Hdr,
        Tonemapping::AcesFitted,
        Bloom { intensity: 0.12, ..Bloom::NATURAL },
        Msaa::Sample4,
        transform,
    )
}

pub struct ScenePlugin;
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DirectionalLightShadowMap { size: 2048 }).add_systems(PostUpdate, apply_quality);
    }
}

/// `?q=low`: no MSAA, bloom or shadows (slow GPUs, software GL).
fn apply_quality(mut low: Local<Option<bool>>, mut commands: Commands, cams: Query<Entity, Added<Camera3d>>, mut suns: Query<&mut DirectionalLight, Added<DirectionalLight>>) {
    if !*low.get_or_insert_with(|| crate::param("q").as_deref() == Some("low")) {
        return;
    }
    for e in cams.iter() {
        commands.entity(e).insert(Msaa::Off).remove::<Bloom>();
    }
    for mut l in suns.iter_mut() {
        l.shadow_maps_enabled = false;
    }
}
