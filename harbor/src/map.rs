use std::collections::VecDeque;
use std::f64::consts::PI;

use bevy::gltf::GltfMaterialName;
use bevy::math::DVec2;
use bevy::mesh::VertexAttributeValues;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use bevy::scene::{SceneInstanceReady, SceneSpawner};
use bevy_water::WaterSettings;

#[cfg(not(target_arch = "wasm32"))]
use bevy::pbr::ExtendedMaterial;
#[cfg(not(target_arch = "wasm32"))]
use bevy_water::material::{StandardWaterMaterial, WaterMaterial};

const TILE_EXTENT: f32 = 4096.0;
const TILE_ZOOM_LEVEL: u32 = 14;
const MAX_CONCURRENT_TILE_LOADS: usize = 16;
const TILE_MANIFEST: TileManifest = TileManifest {
    zoom_level: TILE_ZOOM_LEVEL,
    min_x: 8368,
    max_x: 8400,
    min_y: 5412,
    max_y: 5421,
};

#[derive(Resource, Clone)]
pub(crate) struct TileMaterialPalette {
    building_roof: Handle<StandardMaterial>,
    building_wall: Handle<StandardMaterial>,
    transportation: Handle<StandardMaterial>,
    #[cfg(not(target_arch = "wasm32"))]
    water: Handle<StandardWaterMaterial>,
    #[cfg(target_arch = "wasm32")]
    water: Handle<StandardMaterial>,
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
        label.eq_ignore_ascii_case("water_surface")
    }
}

#[derive(Component)]
pub(crate) struct TileSceneRoot;

#[derive(Component)]
pub(crate) struct PendingTileLoad;

#[derive(Resource)]
pub(crate) struct TileLoadQueue {
    map_root: Entity,
    pending: VecDeque<TileAsset>,
    inflight: usize,
}

#[derive(Resource, Clone, Copy)]
pub struct MapRoot(pub Entity);

#[derive(Resource, Clone, Copy, Debug)]
pub struct TileWorldProjection {
    zoom_level: u32,
    max_tile_x: i32,
    max_tile_y: i32,
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
            ((self.max_tile_x - tile_x.floor() as i32 + 1) as f32 * TILE_EXTENT) - local_x,
            ((self.max_tile_y - tile_y.floor() as i32 + 1) as f32 * TILE_EXTENT) - local_y,
        )
    }

    pub fn lat_lon_to_world(self, latitude: f64, longitude: f64) -> Vec3 {
        let world = self.tile_to_world(self.lat_lon_to_tile(latitude, longitude));
        Vec3::new(world.x, 0.0, world.y)
    }

    fn from_tiles(zoom_level: u32, tiles: &[TileAsset]) -> Self {
        Self {
            zoom_level,
            max_tile_x: tiles.iter().map(|tile| tile.x).max().unwrap_or(0),
            max_tile_y: tiles.iter().map(|tile| tile.y).max().unwrap_or(0),
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
    water_settings: Res<WaterSettings>,
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
        water: materials.add(StandardMaterial {
            base_color: water_settings.shallow_color,
            perceptual_roughness: 0.22,
            metallic: 0.0,
            reflectance: 0.18,
            ..default()
        }),
    });
}

pub fn setup_map_tiles(mut commands: Commands) {
    let tiles = load_tiles(TILE_ZOOM_LEVEL);
    let projection = TileWorldProjection::from_tiles(TILE_ZOOM_LEVEL, &tiles);

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
    commands.insert_resource(TileLoadQueue {
        map_root,
        pending: VecDeque::from(tiles),
        inflight: 0,
    });
}

pub fn spawn_map_tile_batch(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut queue: ResMut<TileLoadQueue>,
) {
    while queue.inflight < MAX_CONCURRENT_TILE_LOADS {
        let Some(tile) = queue.pending.pop_front() else {
            break;
        };

        queue.inflight += 1;
        commands.spawn((
            Name::new(format!("Tile {}_{}", tile.x, tile.y)),
            TileSceneRoot,
            PendingTileLoad,
            ChildOf(queue.map_root),
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(tile.asset_path))),
            Transform::from_translation(tile.translation),
        ));
    }
}

pub fn remap_map_tile_materials(
    trigger: On<SceneInstanceReady>,
    mut commands: Commands,
    scene_spawner: Res<SceneSpawner>,
    tile_roots: Query<(), With<TileSceneRoot>>,
    pending_tile_loads: Query<(), With<PendingTileLoad>>,
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

    if pending_tile_loads.get(root).is_ok() {
        commands.entity(root).remove::<PendingTileLoad>();
        queue.inflight = queue.inflight.saturating_sub(1);
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

            commands.entity(entity).remove::<MeshMaterial3d<StandardMaterial>>();
            #[cfg(not(target_arch = "wasm32"))]
            commands
                .entity(entity)
                .insert(MeshMaterial3d::<StandardWaterMaterial>(palette.water.clone()));
            #[cfg(target_arch = "wasm32")]
            commands
                .entity(entity)
                .insert(MeshMaterial3d::<StandardMaterial>(palette.water.clone()));
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
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
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

    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
}

#[derive(Clone, Debug)]
struct TileAsset {
    x: i32,
    y: i32,
    asset_path: String,
    translation: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct TileManifest {
    zoom_level: u32,
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

fn load_tiles(zoom_level: u32) -> Vec<TileAsset> {
    assert_eq!(
        zoom_level, TILE_MANIFEST.zoom_level,
        "tile manifest only covers zoom level {}",
        TILE_MANIFEST.zoom_level
    );

    let mut tiles = Vec::with_capacity(
        ((TILE_MANIFEST.max_x - TILE_MANIFEST.min_x + 1)
            * (TILE_MANIFEST.max_y - TILE_MANIFEST.min_y + 1)) as usize,
    );

    for x in TILE_MANIFEST.min_x..=TILE_MANIFEST.max_x {
        for y in TILE_MANIFEST.min_y..=TILE_MANIFEST.max_y {
            tiles.push(TileAsset {
                x,
                y,
                asset_path: tile_asset_path(zoom_level, x, y),
                translation: Vec3::new(
                    (TILE_MANIFEST.max_x - x) as f32 * TILE_EXTENT,
                    0.0,
                    (TILE_MANIFEST.max_y - y) as f32 * TILE_EXTENT,
                ),
            });
        }
    }

    tiles
}

fn tile_asset_path(zoom_level: u32, x: i32, y: i32) -> String {
    format!("tiles/{zoom_level}/{x}_{y}.glb")
}

pub fn lat_lon_to_tile(latitude: f64, longitude: f64, zoom_level: u32) -> DVec2 {
    let lat_radians = latitude.to_radians();
    let tiles_per_axis = (1_u64 << zoom_level) as f64;
    let x = (longitude + 180.0) / 360.0 * tiles_per_axis;
    let y =
        (1.0 - ((lat_radians.tan() + 1.0 / lat_radians.cos()).ln() / PI)) * 0.5 * tiles_per_axis;

    DVec2::new(x, y)
}
