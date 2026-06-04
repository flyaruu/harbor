use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use bevy::asset::AssetApp;
use bevy::asset::io::AssetSourceBuilder;
#[cfg(target_arch = "wasm32")]
use bevy::asset::io::wasm::HttpWasmAssetReader;
use bevy::gltf::GltfMaterialName;
use bevy::math::DVec2;
use bevy::mesh::VertexAttributeValues;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use bevy::scene::{SceneInstanceReady, SceneSpawner};
use bevy_egui::EguiContexts;
#[cfg(not(target_arch = "wasm32"))]
use bevy::tasks::{IoTaskPool, Task, block_on, poll_once};
use bevy_panorbit_wasd_camera::PanOrbitCamera;
use bevy_water::WaterSettings;

#[cfg(not(target_arch = "wasm32"))]
use bevy::pbr::ExtendedMaterial;
#[cfg(not(target_arch = "wasm32"))]
use bevy_water::material::{StandardWaterMaterial, WaterMaterial};

const TILE_EXTENT: f32 = 4096.0;
const TILE_ZOOM_LEVEL: u32 = 14;
const MAX_CONCURRENT_TILE_DOWNLOADS: usize = 16;
const MAX_CONCURRENT_TILE_SCENE_LOADS: usize = 4;
const TILE_ANCHOR_LATITUDE: f64 = 51.90189;
const TILE_ANCHOR_LONGITUDE: f64 = 4.49171;
const DEFAULT_TILE_LOAD_RADIUS: i32 = 5;
const TILE_CACHE_ASSET_SOURCE: &str = "tile_cache";
const DEFAULT_TILE_SERVER_URI: &str = "http://localhost:8081";

#[derive(Resource, Clone)]
pub(crate) struct TileMaterialPalette {
    building_roof: Handle<StandardMaterial>,
    building_wall: Handle<StandardMaterial>,
    transportation: Handle<StandardMaterial>,
    #[cfg(not(target_arch = "wasm32"))]
    water: Handle<StandardWaterMaterial>,
    #[cfg(target_arch = "wasm32")]
    water_surface: Handle<StandardMaterial>,
    #[cfg(target_arch = "wasm32")]
    water_volume: Handle<StandardMaterial>,
}

impl TileMaterialPalette {
    fn resolve(&self, label: &str) -> Option<Handle<StandardMaterial>> {
        let normalized = label.to_ascii_lowercase();

        if normalized.contains("building_roof") {
            return Some(self.building_roof.clone());
        }

        if normalized.contains("building_wall") {
            return Some(self.building_wall.clone());
        }

        if normalized.contains("transport") {
            return Some(self.transportation.clone());
        }

        None
    }

    fn is_water(&self, label: &str) -> bool {
        label.to_ascii_lowercase().starts_with("water_")
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn is_water_surface(&self, label: &str) -> bool {
        label.eq_ignore_ascii_case("water_surface")
    }
}

#[derive(Component)]
pub(crate) struct TileSceneRoot;

#[derive(Component, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PendingTileLoad(pub(crate) TileKey);

#[derive(Resource)]
pub(crate) struct TileLoadQueue {
    map_root: Entity,
    ready: VecDeque<TileAsset>,
    inflight_scene_loads: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pending_downloads: VecDeque<TileAsset>,
    #[cfg(not(target_arch = "wasm32"))]
    inflight_downloads: HashMap<TileKey, Task<Result<TileAsset, String>>>,
    states: HashMap<TileKey, TileLoadState>,
    desired: HashSet<TileKey>,
    active_roots: HashMap<TileKey, Entity>,
}

#[derive(Resource, Clone, Copy)]
pub struct MapRoot(pub Entity);

#[derive(Resource, Clone, Copy, Debug)]
pub struct TileLoadRadius(pub i32);

#[derive(Resource, Clone, Copy, Debug)]
pub struct TileWorldProjection {
    zoom_level: u32,
    anchor_tile_x: i32,
    anchor_tile_y: i32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TileKey {
    zoom_level: u32,
    x: i32,
    y: i32,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TileLoadState {
    Unrequested,
    Queued,
    Downloading,
    Cached,
    SceneQueued,
    LoadingScene,
    Loaded,
    Failed,
}

impl TileWorldProjection {
    pub fn lat_lon_to_tile(self, latitude: f64, longitude: f64) -> DVec2 {
        lat_lon_to_tile(latitude, longitude, self.zoom_level)
    }

    pub fn tile_to_world(self, tile: DVec2) -> Vec2 {
        let tile_x = tile.x;
        let tile_y = tile.y;
        let local_x = tile_x.fract() as f32 * TILE_EXTENT;
        let local_y = tile_y.fract() as f32 * TILE_EXTENT;

        Vec2::new(
            ((self.anchor_tile_x - tile_x.floor() as i32 + 1) as f32 * TILE_EXTENT) - local_x,
            ((self.anchor_tile_y - tile_y.floor() as i32 + 1) as f32 * TILE_EXTENT) - local_y,
        )
    }

    pub fn lat_lon_to_world(self, latitude: f64, longitude: f64) -> Vec3 {
        let world = self.tile_to_world(self.lat_lon_to_tile(latitude, longitude));
        Vec3::new(world.x, 0.0, world.y)
    }

    pub fn world_to_tile(self, world: Vec3) -> DVec2 {
        DVec2::new(
            self.anchor_tile_x as f64 + 1.0 - (world.x as f64 / TILE_EXTENT as f64),
            self.anchor_tile_y as f64 + 1.0 - (world.z as f64 / TILE_EXTENT as f64),
        )
    }

    fn new(zoom_level: u32, anchor_tile_x: i32, anchor_tile_y: i32) -> Self {
        Self {
            zoom_level,
            anchor_tile_x,
            anchor_tile_y,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn setup_map_tile_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut extended_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, WaterMaterial>>>,
    water_settings: Res<WaterSettings>,
) {
    let water_material = StandardWaterMaterial {
        base: StandardMaterial {
            base_color: water_settings.base_color,
            alpha_mode: water_settings.alpha_mode,
            perceptual_roughness: 0.22,
            ..default()
        },
        extension: WaterMaterial {
            amplitude: water_settings.amplitude,
            clarity: water_settings.clarity,
            deep_color: water_settings.deep_color,
            shallow_color: water_settings.shallow_color,
            edge_color: water_settings.edge_color,
            edge_scale: water_settings.edge_scale,
            coord_scale: Vec2::ONE,
            quality: water_settings.water_quality.into(),
            ..default()
        },
    };

    commands.insert_resource(TileMaterialPalette {
        building_roof: materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.09, 0.08),
            perceptual_roughness: 0.95,
            metallic: 0.0,
            reflectance: 0.18,
            ..default()
        }),
        building_wall: materials.add(StandardMaterial {
            base_color: Color::srgb(0.23, 0.16, 0.11),
            perceptual_roughness: 0.97,
            metallic: 0.0,
            reflectance: 0.18,
            ..default()
        }),
        transportation: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.12, 0.13),
            perceptual_roughness: 0.98,
            metallic: 0.0,
            reflectance: 0.18,
            ..default()
        }),
        water: extended_materials.add(water_material),
    });
}

#[cfg(target_arch = "wasm32")]
pub fn setup_map_tile_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    _water_settings: Res<WaterSettings>,
) {
    commands.insert_resource(TileMaterialPalette {
        building_roof: materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.09, 0.08),
            perceptual_roughness: 0.95,
            metallic: 0.0,
            reflectance: 0.18,
            ..default()
        }),
        building_wall: materials.add(StandardMaterial {
            base_color: Color::srgb(0.23, 0.16, 0.11),
            perceptual_roughness: 0.97,
            metallic: 0.0,
            reflectance: 0.18,
            ..default()
        }),
        transportation: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.12, 0.13),
            perceptual_roughness: 0.98,
            metallic: 0.0,
            reflectance: 0.18,
            ..default()
        }),
        water_surface: materials.add(StandardMaterial {
            // Keep the wasm fallback intentionally simple and stable.
            base_color: Color::srgb(0.08, 0.32, 0.66),
            alpha_mode: AlphaMode::Blend,
            emissive: LinearRgba::rgb(0.02, 0.08, 0.16),
            perceptual_roughness: 1.0,
            metallic: 0.0,
            reflectance: 0.0,
            unlit: true,
            ..default()
        }),
        water_volume: materials.add(StandardMaterial {
            base_color: Color::srgb(0.03, 0.14, 0.28),
            emissive: LinearRgba::rgb(0.01, 0.03, 0.06),
            perceptual_roughness: 1.0,
            metallic: 0.0,
            reflectance: 0.0,
            unlit: true,
            cull_mode: None,
            ..default()
        }),
    });
}

pub fn setup_map_tiles(mut commands: Commands) {
    let anchor_tile = lat_lon_to_tile(TILE_ANCHOR_LATITUDE, TILE_ANCHOR_LONGITUDE, TILE_ZOOM_LEVEL);
    let projection = TileWorldProjection::new(
        TILE_ZOOM_LEVEL,
        anchor_tile.x.floor() as i32,
        anchor_tile.y.floor() as i32,
    );

    let map_root = commands
        .spawn((
            Name::new("Map Root"),
            Transform::default(),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    commands.insert_resource(projection);
    commands.insert_resource(MapRoot(map_root));
    commands.insert_resource(TileLoadRadius(DEFAULT_TILE_LOAD_RADIUS));
    commands.insert_resource(TileLoadQueue {
        map_root,
        #[cfg(target_arch = "wasm32")]
        ready: VecDeque::new(),
        #[cfg(not(target_arch = "wasm32"))]
        ready: VecDeque::new(),
        inflight_scene_loads: 0,
        #[cfg(not(target_arch = "wasm32"))]
        pending_downloads: VecDeque::new(),
        #[cfg(not(target_arch = "wasm32"))]
        inflight_downloads: HashMap::new(),
        states: HashMap::new(),
        desired: HashSet::new(),
        active_roots: HashMap::new(),
    });
}

pub fn adjust_tile_load_radius(
    input: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    mut tile_load_radius: ResMut<TileLoadRadius>,
) {
    let ctx = contexts.ctx_mut().expect("primary egui context");
    if ctx.wants_keyboard_input() {
        return;
    }

    let mut next_radius = tile_load_radius.0;
    if input.just_pressed(KeyCode::BracketLeft) {
        next_radius = next_radius.saturating_sub(1);
    }
    if input.just_pressed(KeyCode::BracketRight) {
        next_radius = next_radius.saturating_add(1);
    }

    if next_radius != tile_load_radius.0 {
        tile_load_radius.0 = next_radius;
        info!(tile_load_radius = next_radius, "updated tile load radius");
    }
}

pub fn update_desired_map_tiles(
    mut commands: Commands,
    projection: Res<TileWorldProjection>,
    camera: Single<&PanOrbitCamera, With<Camera3d>>,
    tile_load_radius: Res<TileLoadRadius>,
    mut queue: ResMut<TileLoadQueue>,
) {
    let desired = desired_tiles_for_focus(&projection, camera.focus, tile_load_radius.0);

    let to_unload = queue
        .active_roots
        .iter()
        .filter_map(|(key, entity)| (!desired.contains(key)).then_some((*key, *entity)))
        .collect::<Vec<_>>();
    for (key, entity) in to_unload {
        commands.entity(entity).despawn();
        queue.active_roots.remove(&key);
        if matches!(queue.states.get(&key), Some(TileLoadState::LoadingScene)) {
            queue.inflight_scene_loads = queue.inflight_scene_loads.saturating_sub(1);
        }
        queue.states.insert(key, TileLoadState::Cached);
    }

    for key in desired.iter().copied() {
        let tile = tile_asset_for_key(*projection, key);
        let state = queue
            .states
            .get(&key)
            .copied()
            .unwrap_or(TileLoadState::Unrequested);
        match state {
            TileLoadState::Unrequested => {
                #[cfg(target_arch = "wasm32")]
                {
                    queue.ready.push_back(tile);
                    queue.states.insert(key, TileLoadState::SceneQueued);
                }

                #[cfg(not(target_arch = "wasm32"))]
                queue.pending_downloads.push_back(tile);
                #[cfg(not(target_arch = "wasm32"))]
                queue.states.insert(key, TileLoadState::Queued);
            }
            TileLoadState::Cached if !queue.active_roots.contains_key(&key) => {
                queue.ready.push_back(tile);
                queue.states.insert(key, TileLoadState::SceneQueued);
            }
            TileLoadState::Loaded if !queue.active_roots.contains_key(&key) => {
                queue.states.insert(key, TileLoadState::SceneQueued);
                queue.ready.push_back(tile);
            }
            TileLoadState::Failed
            | TileLoadState::Queued
            | TileLoadState::Downloading
            | TileLoadState::SceneQueued
            | TileLoadState::LoadingScene
            | TileLoadState::Cached
            | TileLoadState::Loaded => {}
        }
    }

    queue.desired = desired;
}

#[cfg(not(target_arch = "wasm32"))]
pub fn download_map_tile_batch(mut queue: ResMut<TileLoadQueue>) {
    while queue.inflight_downloads.len() < MAX_CONCURRENT_TILE_DOWNLOADS {
        let Some(tile) = queue.pending_downloads.pop_front() else {
            break;
        };

        let key = tile.key();
        if !queue.desired.contains(&key) {
            queue.states.insert(key, TileLoadState::Unrequested);
            continue;
        }
        let cache_path = tile_cache_path(&tile);
        if cache_path.exists() {
            queue.ready.push_back(tile);
            queue.states.insert(key, TileLoadState::Cached);
            continue;
        }

        let task = IoTaskPool::get().spawn(async move { cache_tile_from_http(tile) });
        queue.states.insert(key, TileLoadState::Downloading);
        queue.inflight_downloads.insert(key, task);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn collect_completed_map_tile_downloads(mut queue: ResMut<TileLoadQueue>) {
    let completed = queue
        .inflight_downloads
        .iter_mut()
        .filter_map(|(key, task)| block_on(poll_once(task)).map(|result| (*key, result)))
        .collect::<Vec<_>>();

    for (key, result) in completed {
        queue.inflight_downloads.remove(&key);
        match result {
            Ok(tile) => {
                queue.states.insert(key, TileLoadState::Cached);
                queue.ready.push_back(tile);
            }
            Err(error) => {
                queue.states.insert(key, TileLoadState::Failed);
                warn!(zoom = key.zoom_level, x = key.x, y = key.y, "{error}");
            }
        }
    }
}

pub fn spawn_map_tile_batch(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut queue: ResMut<TileLoadQueue>,
) {
    while queue.inflight_scene_loads < MAX_CONCURRENT_TILE_SCENE_LOADS {
        let Some(tile) = queue.ready.pop_front() else {
            break;
        };

        let key = tile.key();
        if !queue.desired.contains(&key) || queue.active_roots.contains_key(&key) {
            queue.states.insert(key, TileLoadState::Cached);
            continue;
        }

        queue.inflight_scene_loads += 1;
        queue.states.insert(key, TileLoadState::LoadingScene);
        let root = commands
            .spawn((
                Name::new(format!("Tile {}_{}", tile.x, tile.y)),
                TileSceneRoot,
                PendingTileLoad(key),
                ChildOf(queue.map_root),
                SceneRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset(tile.asset_path.clone())),
                ),
                Transform::from_translation(tile.translation),
            ))
            .id();
        queue.active_roots.insert(key, root);
    }
}

pub fn remap_map_tile_materials(
    trigger: On<SceneInstanceReady>,
    mut commands: Commands,
    scene_spawner: Res<SceneSpawner>,
    tile_roots: Query<(), With<TileSceneRoot>>,
    pending_tile_loads: Query<&PendingTileLoad>,
    palette: Res<TileMaterialPalette>,
    mut queue: ResMut<TileLoadQueue>,
    mesh_handles: Query<&Mesh3d>,
    transforms: Query<&GlobalTransform>,
    names: Query<&Name>,
    material_names: Query<&GltfMaterialName>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let root = trigger.event().entity;

    if tile_roots.get(root).is_err() {
        return;
    }

    if let Ok(pending) = pending_tile_loads.get(root) {
        commands.entity(root).remove::<PendingTileLoad>();
        queue.inflight_scene_loads = queue.inflight_scene_loads.saturating_sub(1);
        if queue.desired.contains(&pending.0) {
            queue.states.insert(pending.0, TileLoadState::Loaded);
        } else {
            queue.states.insert(pending.0, TileLoadState::Cached);
            queue.active_roots.remove(&pending.0);
            commands.entity(root).despawn();
            return;
        }
    }

    for entity in scene_spawner.iter_instance_entities(trigger.event().instance_id) {
        let material_name = material_names.get(entity).ok().map(|name| name.0.as_str());
        let entity_name = names.get(entity).ok().map(|name| name.as_str());

        if material_name.is_some_and(|label| palette.is_water(label))
            || entity_name.is_some_and(|label| palette.is_water(label))
        {
            if let Ok(mesh_handle) = mesh_handles.get(entity)
                && let Ok(transform) = transforms.get(entity)
                && let Some(mesh) = meshes.get_mut(mesh_handle)
            {
                ensure_water_uvs(mesh, transform);
            }

            commands
                .entity(entity)
                .remove::<MeshMaterial3d<StandardMaterial>>();
            #[cfg(not(target_arch = "wasm32"))]
            commands
                .entity(entity)
                .insert(MeshMaterial3d::<StandardWaterMaterial>(
                    palette.water.clone(),
                ));
            #[cfg(target_arch = "wasm32")]
            commands
                .entity(entity)
                .insert(MeshMaterial3d::<StandardMaterial>(
                    if material_name.is_some_and(|label| palette.is_water_surface(label))
                        || entity_name.is_some_and(|label| palette.is_water_surface(label))
                    {
                        palette.water_surface.clone()
                    } else {
                        palette.water_volume.clone()
                    },
                ));
            continue;
        }

        let override_material = material_name
            .and_then(|label| palette.resolve(label))
            .or_else(|| entity_name.and_then(|label| palette.resolve(label)));

        if let Some(override_material) = override_material
            && let Ok(mut mesh_material) = standard_materials.get_mut(entity)
        {
            mesh_material.0 = override_material;
        }
    }
}

fn ensure_water_uvs(mesh: &mut Mesh, global_transform: &GlobalTransform) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };

    let affine = global_transform.affine();
    let uvs = positions
        .iter()
        .map(|[x, y, z]| {
            let world_position = affine.transform_point3(Vec3::new(*x, *y, *z));
            [world_position.x, world_position.z]
        })
        .collect::<Vec<_>>();

    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
}

#[derive(Clone, Debug)]
struct TileAsset {
    zoom_level: u32,
    x: i32,
    y: i32,
    asset_path: String,
    translation: Vec3,
}

impl TileAsset {
    fn key(&self) -> TileKey {
        TileKey {
            zoom_level: self.zoom_level,
            x: self.x,
            y: self.y,
        }
    }
}

fn tile_asset_for_key(projection: TileWorldProjection, key: TileKey) -> TileAsset {
    TileAsset {
        zoom_level: key.zoom_level,
        x: key.x,
        y: key.y,
        asset_path: tile_asset_path(key.zoom_level, key.x, key.y),
        translation: Vec3::new(
            (projection.anchor_tile_x - key.x) as f32 * TILE_EXTENT,
            0.0,
            (projection.anchor_tile_y - key.y) as f32 * TILE_EXTENT,
        ),
    }
}

fn desired_tiles_for_focus(
    projection: &TileWorldProjection,
    focus: Vec3,
    tile_load_radius: i32,
) -> HashSet<TileKey> {
    let center = projection.world_to_tile(focus);
    let center_x = center.x.floor() as i32;
    let center_y = center.y.floor() as i32;
    let mut desired = HashSet::new();

    for x in (center_x - tile_load_radius)..=(center_x + tile_load_radius) {
        for y in (center_y - tile_load_radius)..=(center_y + tile_load_radius) {
            desired.insert(TileKey {
                zoom_level: projection.zoom_level,
                x,
                y,
            });
        }
    }

    desired
}

fn tile_asset_path(zoom_level: u32, x: i32, y: i32) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{TILE_CACHE_ASSET_SOURCE}://{zoom_level}/{x}_{y}.glb")
    }

    #[cfg(target_arch = "wasm32")]
    {
        format!("{TILE_CACHE_ASSET_SOURCE}://{zoom_level}/{x}/{y}.glb")
    }
}

pub fn configure_tile_asset_source(app: &mut App) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let cache_root = tile_cache_root();
        if let Err(error) = fs::create_dir_all(&cache_root) {
            warn!(path = %cache_root.display(), "failed to create tile cache directory: {error}");
        }
        let asset_root = cache_root.to_string_lossy().into_owned();
        app.register_asset_source(
            TILE_CACHE_ASSET_SOURCE,
            AssetSourceBuilder::platform_default(&asset_root, None),
        );
    }

    #[cfg(target_arch = "wasm32")]
    {
        let root = format!("{}/data", tile_server_uri().trim_end_matches('/'));
        app.register_asset_source(
            TILE_CACHE_ASSET_SOURCE,
            AssetSourceBuilder::new(move || Box::new(HttpWasmAssetReader::new(root.clone()))),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tile_cache_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("harbor crate should live inside the workspace root")
        .join(".cache/harbor_tiles")
}

#[cfg(not(target_arch = "wasm32"))]
fn tile_cache_relative_path(tile: &TileAsset) -> PathBuf {
    PathBuf::from(tile.zoom_level.to_string()).join(format!("{}_{}.glb", tile.x, tile.y))
}

#[cfg(not(target_arch = "wasm32"))]
fn tile_cache_path(tile: &TileAsset) -> PathBuf {
    tile_cache_root().join(tile_cache_relative_path(tile))
}

#[cfg(not(target_arch = "wasm32"))]
fn tile_server_uri() -> String {
    if let Some(uri) = crate::runtime::native_cli_tile_server_uri() {
        return uri;
    }

    std::env::var("TILE_SERVER_URI").unwrap_or_else(|_| DEFAULT_TILE_SERVER_URI.to_string())
}

#[cfg(target_arch = "wasm32")]
fn tile_server_uri() -> String {
    runtime_config_value("tile_server_uri").unwrap_or_else(|| DEFAULT_TILE_SERVER_URI.to_string())
}

#[cfg(target_arch = "wasm32")]
fn runtime_config_value(browser_key: &str) -> Option<String> {
    crate::runtime::browser_runtime_url_value(browser_key)
}

#[cfg(not(target_arch = "wasm32"))]
fn tile_request_url(tile: &TileAsset) -> String {
    format!(
        "{}/data/{}/{}/{}.glb",
        tile_server_uri().trim_end_matches('/'),
        tile.zoom_level,
        tile.x,
        tile.y
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_tile_from_http(tile: TileAsset) -> Result<TileAsset, String> {
    let url = tile_request_url(&tile);
    let cache_path = tile_cache_path(&tile);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create tile cache directory {}: {error}",
                parent.display()
            )
        })?;
    }

    let response = ureq::get(&url).call().map_err(|error| {
        format!(
            "failed to fetch tile {}_{} from {url}: {error}",
            tile.x, tile.y
        )
    })?;
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read tile response from {url}: {error}"))?;
    fs::write(&cache_path, bytes).map_err(|error| {
        format!(
            "failed to write cached tile {}_{} to {}: {error}",
            tile.x,
            tile.y,
            cache_path.display()
        )
    })?;
    Ok(tile)
}

pub fn lat_lon_to_tile(latitude: f64, longitude: f64, zoom_level: u32) -> DVec2 {
    let lat_radians = latitude.to_radians();
    let tiles_per_axis = (1_u64 << zoom_level) as f64;
    let x = (longitude + 180.0) / 360.0 * tiles_per_axis;
    let y =
        (1.0 - ((lat_radians.tan() + 1.0 / lat_radians.cos()).ln() / PI)) * 0.5 * tiles_per_axis;

    DVec2::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn builds_tile_request_url() {
        let tile = TileAsset {
            zoom_level: 14,
            x: 8396,
            y: 5421,
            asset_path: String::new(),
            translation: Vec3::ZERO,
        };

        assert_eq!(
            tile_request_url(&tile),
            "http://localhost:8081/data/14/8396/5421.glb"
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn builds_cache_relative_path() {
        let tile = TileAsset {
            zoom_level: 14,
            x: 8396,
            y: 5421,
            asset_path: String::new(),
            translation: Vec3::ZERO,
        };

        assert_eq!(
            tile_cache_relative_path(&tile),
            PathBuf::from("14/8396_5421.glb")
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn computes_desired_tiles_around_focus() {
        let anchor_tile =
            lat_lon_to_tile(TILE_ANCHOR_LATITUDE, TILE_ANCHOR_LONGITUDE, TILE_ZOOM_LEVEL);
        let projection = TileWorldProjection::new(
            TILE_ZOOM_LEVEL,
            anchor_tile.x.floor() as i32,
            anchor_tile.y.floor() as i32,
        );
        let focus = projection.lat_lon_to_world(51.90189, 4.49171);

        let desired = desired_tiles_for_focus(&projection, focus, DEFAULT_TILE_LOAD_RADIUS);

        assert!(!desired.is_empty());
        assert!(desired.contains(&TileKey {
            zoom_level: TILE_ZOOM_LEVEL,
            x: anchor_tile.x.floor() as i32,
            y: anchor_tile.y.floor() as i32,
        }));
    }
}
