//! Shared scene dressing: golden-hour lights, gradient sky + clouds, and the
//! camera "look" (HDR, ACES, bloom, MSAA).
use crate::models::{self, Geo};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::camera::Hdr;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
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
            sky: Color::srgb_u8(0x7c, 0xa4, 0xf0),
            ground: Color::srgb_u8(0xc0, 0x8a, 0x58),
            sun: Color::srgb_u8(0xff, 0xc9, 0x85),
            intensity: 1.0,
            dir: Vec3::new(-6.0, 12.0, -5.0),
            size: 14.0,
        }
    }
}

/// Golden hour: a strong warm key light, a cool sky fill so shadows go blue,
/// and a rim light opposite the sun that outlines silhouettes.
pub fn sun_lights(commands: &mut Commands, o: SunOpts, scope: impl Bundle + Clone) {
    commands.spawn((
        DirectionalLight { color: o.sun, illuminance: 11_000.0 * o.intensity, shadow_maps_enabled: true, ..default() },
        Transform::from_translation(o.dir).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: o.size * 3.5, first_cascade_far_bound: o.size * 3.5, ..default() }.build(),
        Name::new("Sun"),
        scope.clone(),
    ));
    // WebGL2 allows only one directional light, so the rim is a distant point
    // light: across from the sun, which is behind the subject for our cameras
    let d = o.size * 1.5;
    let rim = Vec3::new(-o.dir.x, o.dir.y * 0.6, -o.dir.z).normalize() * d;
    commands.spawn((
        Name::new("Rim"),
        PointLight { color: Color::srgb_u8(0xff, 0xe6, 0xc4), intensity: 35_000.0 * d * d * o.intensity, range: d * 2.5, shadow_maps_enabled: false, ..default() },
        Transform::from_translation(rim),
        scope,
    ));
    // a little warm ground bounce in a mostly sky-blue fill
    let fill = models::col_of(o.sky).lerp(models::col_of(o.ground), 0.1);
    commands.insert_resource(GlobalAmbientLight { color: models::to_color(fill), brightness: 1000.0 * o.intensity, ..default() });
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
    // blue from just above the horizon; the pale band stays thin
    let col = |v: Vec3| b.lerp(t, (v.y * 2.6 + 0.25).clamp(0.0, 1.0));
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
        Name::new("Sky"),
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
            Name::new("Cloud"),
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
        Name::new("Camera"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: fov_deg.to_radians(), near: 0.1, far: 400.0, ..default() }),
        Hdr,
        Tonemapping::AcesFitted,
        // ACES mutes saturated colours; push them back toward the painted look
        ColorGrading {
            global: ColorGradingGlobal { post_saturation: 1.15, ..default() },
            midtones: ColorGradingSection { contrast: 1.08, ..default() },
            ..default()
        },
        Bloom { intensity: 0.12, ..Bloom::NATURAL },
        Msaa::Sample4,
        transform,
    )
}

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(DirectionalLightShadowMap { size: 2048 });
    app.insert_resource(LowQuality(crate::params::param("q").as_deref() == Some("low")));
    app.add_systems(PostUpdate, apply_low_quality.run_if(|q: Res<LowQuality>| q.0));
    app.add_observer(fog_everywhere);
}

/// Every camera gets distance fog (past the far plane where a screen doesn't
/// set its own): fog is a shader define, so mixing fogged and unfogged
/// cameras would compile every pipeline twice.
fn fog_everywhere(add: On<Add<Camera3d>>, mut commands: Commands) {
    commands.entity(add.entity).insert_if_new(DistanceFog { falloff: FogFalloff::Linear { start: 1000.0, end: 2000.0 }, ..default() });
}

/// `?q=low`: no MSAA, bloom or shadows (slow GPUs, software GL).
#[derive(Resource)]
struct LowQuality(bool);

fn apply_low_quality(mut commands: Commands, cams: Query<Entity, Added<Camera3d>>, mut suns: Query<&mut DirectionalLight, Added<DirectionalLight>>) {
    for e in cams.iter() {
        commands.entity(e).insert(Msaa::Off).remove::<Bloom>();
    }
    for mut l in suns.iter_mut() {
        l.shadow_maps_enabled = false;
    }
}
