//! Blender-made models (`art/*.py` -> `assets/models/*.glb`).
//!
//! Every glb is loaded up front behind [`Screen::Loading`], then split into
//! named [`Piece`]s keyed `"<asset>.<node>"` (e.g. `"wheel.spin"`), so the rest
//! of the game can spawn them synchronously like any other mesh.
//!
//! A piece is one mesh + its asset's baked material, plus the node's rest
//! transform within the asset (a wheel's spin pivot, a hammer's arm pivot).
//! Tint slots — surfaces baked neutral and tagged with a vertex-colour marker —
//! take a colour at spawn time ([`Art::mesh`]), e.g. a bot's paint.
use crate::models::Col;
use crate::screens::Screen;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode};
use bevy::pbr::gltf::standard_material_from_gltf_material;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::sync::Mutex;

/// Every glb under assets/models, by file stem.
pub const ASSETS: &[&str] = &[
    // robot parts
    "core", "block", "slab", "girder", "armour", "battery", "wheel", "bigwheel", "leg", "spinner", "hammer", "flipper", "spike", "wedge",
    // workshop
    "ws_turntable", "ws_floor", "ws_wall_back", "ws_wall_left", "ws_wall_right", "ws_wall_front", "ws_bench", "ws_props", "ws_corners",
    // junk props and the scrapyard
    "barrel", "crate", "tyre", "car_shell", "washer", "fridge", "engine", "pipe", "sheet", "girder_bit", "plate", "bolts", "bits",
    "yard_ground", "yard_outer", "fence", "mound", "mountain", "car_stack", "zone_board", "van", "cabin", "lamp_post", "bunting",
    "kennel", "warning_sign", "crane", "scavenger", "dog", "tarp",
    // arena
    "arena_tile", "arena_marks", "arena_pit", "arena_wall", "arena_stand", "arena_tower", "arena_banner", "arena_backwall", "arena_barrier",
    "arena_cone", "crowd_person", "arena_ground",
];

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, start_loading);
    app.add_systems(Update, (finish_loading, warm_up).chain().run_if(in_state(Screen::Loading)));
}

#[derive(Resource)]
struct Loading(Vec<(&'static str, Handle<Gltf>)>);

fn start_loading(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Loading(ASSETS.iter().map(|n| (*n, assets.load(format!("models/{n}.glb")))).collect()));
}

/// Tint slot per vertex; `NONE` for untinted.
const NONE: u8 = u8::MAX;

pub struct Piece {
    pub mesh: Handle<Mesh>,
    pub mat: Handle<StandardMaterial>,
    /// darkened, battered copy of `mat` for scorched / worn parts
    pub scorch: Handle<StandardMaterial>,
    /// the node's rest transform within its asset (its pivot)
    pub xf: Transform,
    /// vertex positions in the piece's own frame
    pub pts: Vec<Vec3>,
    slots: Vec<u8>,
}

#[derive(Resource)]
pub struct Art {
    pieces: HashMap<String, Piece>,
    tinted: Mutex<HashMap<(String, [[u8; 3]; 3]), Handle<Mesh>>>,
}

impl Art {
    pub fn get(&self, key: &str) -> Option<&Piece> {
        self.pieces.get(key)
    }

    pub fn piece(&self, key: &str) -> &Piece {
        self.pieces.get(key).unwrap_or_else(|| panic!("no art piece {key:?}"))
    }

    /// The piece's mesh with its tint slots coloured (slot 0, 1, 2 in order).
    /// Untinted pieces ignore `tints`. Tinted variants are cached.
    pub fn mesh(&self, meshes: &mut Assets<Mesh>, key: &str, tints: &[Col]) -> Handle<Mesh> {
        let p = self.piece(key);
        if p.slots.is_empty() {
            return p.mesh.clone();
        }
        let mut k = [[255u8; 3]; 3];
        for (i, c) in tints.iter().take(3).enumerate() {
            let s = Color::linear_rgb(c.x, c.y, c.z).to_srgba().to_u8_array();
            k[i] = [s[0], s[1], s[2]];
        }
        let mut cache = self.tinted.lock().unwrap();
        cache
            .entry((key.to_string(), k))
            .or_insert_with(|| {
                let mut m = meshes.get(&p.mesh).expect("art mesh loaded").clone();
                let cols: Vec<[f32; 4]> = p
                    .slots
                    .iter()
                    .map(|&s| match tints.get(s as usize) {
                        Some(c) if s != NONE => [c.x, c.y, c.z, 1.0],
                        _ => [1.0; 4],
                    })
                    .collect();
                m.insert_attribute(Mesh::ATTRIBUTE_COLOR, cols);
                meshes.add(m)
            })
            .clone()
    }

    /// Mesh, material and transform (`t` * the piece's rest pose) for a piece.
    pub fn bundle(&self, meshes: &mut Assets<Mesh>, key: &str, tints: &[Col], t: Transform) -> (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform) {
        let p = self.piece(key);
        (Mesh3d(self.mesh(meshes, key, tints)), MeshMaterial3d(p.mat.clone()), t * p.xf)
    }

    /// Like [`Art::bundle`] but at `t` alone, ignoring the piece's rest pose:
    /// for modular pieces that sit side by side in their file only for previews.
    pub fn place(&self, meshes: &mut Assets<Mesh>, key: &str, tints: &[Col], t: Transform) -> (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform) {
        (Mesh3d(self.mesh(meshes, key, tints)), MeshMaterial3d(self.piece(key).mat.clone()), t)
    }

    /// Spawn every piece of `asset` at its rest pose under `t`; glow pieces
    /// (`glow_*`) get the material `glow` picks for their name.
    pub fn spawn_all(&self, commands: &mut Commands, meshes: &mut Assets<Mesh>, asset: &str, tints: &[Col], t: Transform, mut glow: impl FnMut(&str) -> Handle<StandardMaterial>, extra: impl Bundle + Clone) {
        let prefix = format!("{asset}.");
        let mut keys: Vec<&String> = self.pieces.keys().filter(|k| k.starts_with(&prefix)).collect();
        keys.sort();
        for k in keys {
            let node = &k[prefix.len()..];
            if node.starts_with("glow_") {
                commands.spawn((self.glow(k, glow(node), t), extra.clone()));
            } else {
                commands.spawn((self.bundle(meshes, k, tints, t), extra.clone()));
            }
        }
    }

    /// A glowing piece (eyes, lamps) with the given material instead of its own.
    pub fn glow(&self, key: &str, mat: Handle<StandardMaterial>, t: Transform) -> (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform, bevy::light::NotShadowCaster) {
        let p = self.piece(key);
        (Mesh3d(p.mesh.clone()), MeshMaterial3d(mat), t * p.xf, bevy::light::NotShadowCaster)
    }
}

fn finish_loading(
    mut commands: Commands,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    gmeshes: Res<Assets<GltfMesh>>,
    gmats: Res<Assets<GltfMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    loading: Option<Res<Loading>>,
) {
    let Some(loading) = loading else { return };
    for (name, h) in &loading.0 {
        if let Some(bevy::asset::LoadState::Failed(e)) = assets.get_load_state(h) {
            panic!("failed to load models/{name}.glb: {e}");
        }
        if !assets.is_loaded_with_dependencies(h) {
            return;
        }
    }
    let mut pieces = HashMap::default();
    for (name, h) in &loading.0 {
        let gltf = gltfs.get(h).unwrap();
        let mut by_mat: HashMap<AssetId<GltfMaterial>, (Handle<StandardMaterial>, Handle<StandardMaterial>)> = HashMap::default();
        for nh in &gltf.nodes {
            let node = nodes.get(nh).unwrap();
            let Some(gm) = node.mesh.as_ref().and_then(|m| gmeshes.get(m)) else { continue };
            let prim = &gm.primitives[0];
            let (mat, scorch) = match &prim.material {
                Some(mh) => by_mat
                    .entry(mh.id())
                    .or_insert_with(|| {
                        let mut m = standard_material_from_gltf_material(gmats.get(mh).unwrap());
                        // baked textures already carry the light and wear; keep highlights soft
                        m.reflectance = 0.35;
                        let mut s = m.clone();
                        s.base_color = Color::srgb_u8(0x8a, 0x80, 0x7a);
                        s.perceptual_roughness = 1.0;
                        (mats.add(m), mats.add(s))
                    })
                    .clone(),
                None => (Handle::default(), Handle::default()),
            };
            let mut mesh = meshes.get_mut(&prim.mesh).unwrap();
            let pts: Vec<Vec3> = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap_or(&[]).iter().map(|p| Vec3::from(*p)).collect();
            let mut slots = vec![];
            if let Some(cols) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
                let cols: Vec<[f32; 4]> = match cols {
                    bevy::mesh::VertexAttributeValues::Float32x4(v) => v.clone(),
                    _ => panic!("models/{name}.glb {}: unexpected vertex colour format", node.name),
                };
                // white = untinted; otherwise the marker's strongest channel is the slot
                slots = cols
                    .iter()
                    .map(|c| if c[0].min(c[1]).min(c[2]) > 0.5 { NONE } else { (0..3).max_by(|&a, &b| c[a].total_cmp(&c[b])).unwrap() as u8 })
                    .collect();
                // untinted use shows the neutral bake
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; cols.len()]);
            }
            *mesh = crate::models::uniform_layout(mesh.clone());
            pieces.insert(format!("{name}.{}", node.name), Piece { mesh: prim.mesh.clone(), mat, scorch, xf: node.transform, pts, slots });
        }
    }
    let art = Art { pieces, tinted: Mutex::new(HashMap::default()) };
    spawn_warm_up(&mut commands, &mut meshes, &mut mats, &art);
    commands.insert_resource(art);
    commands.remove_resource::<Loading>();
}

/// The warm-up scene: one of every pipeline the game draws with (opaque,
/// glow, sky, blended fx, ghosts and rings; shadows; the shared camera look),
/// shown for a few frames while loading so WebGL compiles them all up front
/// instead of freezing on the first visit to each screen.
#[derive(Resource)]
struct WarmUp(u32);

fn spawn_warm_up(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, art: &Art) {
    use crate::scene;
    commands.insert_resource(WarmUp(0));
    let scope = DespawnOnExit(Screen::Loading);
    scene::sun_lights(commands, scene::SunOpts::default(), scope.clone());
    commands.spawn((scene::camera_bundle(40.0, Transform::from_xyz(0.0, 0.5, 3.0).looking_at(Vec3::ZERO, Vec3::Y)), scope.clone()));
    let mesh = meshes.add(crate::models::cyl(0.2, 0.2, 0.2, 8, Vec3::ONE, 0.0).to_mesh());
    let flavours = [
        StandardMaterial { unlit: true, ..default() },
        StandardMaterial { unlit: true, cull_mode: None, fog_enabled: false, ..default() },
        StandardMaterial { alpha_mode: AlphaMode::Blend, ..default() },
        StandardMaterial { unlit: true, alpha_mode: AlphaMode::Blend, ..default() },
        StandardMaterial { unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() },
    ];
    for (i, m) in flavours.into_iter().enumerate() {
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mats.add(m)), Transform::from_xyz(i as f32 * 0.3 - 0.6, 0.0, 0.0), scope.clone()));
    }
    // a real textured, tinted piece casting a shadow onto a plain one
    commands.spawn((art.bundle(meshes, "core.body", &[Vec3::ONE], Transform::from_xyz(0.0, 0.4, 0.0)), scope.clone()));
    commands.spawn((art.bundle(meshes, "arena_tile.a", &[], Transform::IDENTITY), scope));
}

fn warm_up(mut commands: Commands, w: Option<ResMut<WarmUp>>, mut next: ResMut<NextState<Screen>>) {
    let Some(mut w) = w else { return };
    w.0 += 1;
    if w.0 > 4 {
        commands.remove_resource::<WarmUp>();
        next.set(crate::screens::first_screen());
    }
}
