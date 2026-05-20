use bevy::camera::primitives::MeshAabb;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::scene::{SceneInstanceReady, SceneSpawner};
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use bevy_water::WaterSettings;
use spacetimedb_sdk::Table;
use std::path::Path;

use crate::map::{MapRoot, TileWorldProjection};
use crate::module_bindings::{
    LocationReport, LocationReportTableAccess, Ship as ShipRecord, ShipTableAccess,
};
use crate::ship_class::ShipClass;
use crate::spacetime::StdbConn;
use crate::ui::ShipInfoOverlay;

const SHIP_HEADING: f32 = 0.0;
const SHIP_HEADING_SMOOTHING_RATE: f32 = 6.0;
const SHIP_PICK_RADIUS: f32 = 45.0;
const SHIP_FOOTPRINT_HEIGHT: f32 = 4.0;
const SHIP_FOOTPRINT_Y_OFFSET: f32 = -5.5;
const ROUTE_HEIGHT: f32 = 5.0;
const ROUTE_WIDTH: f32 = 20.0;

#[derive(Component, Clone, Copy)]
pub struct Ship {
    pub lat: f64,
    pub lon: f64,
    pub cog: Option<f64>,
    pub sog: Option<f64>,
    pub heading: f32,
}

impl Ship {
    pub fn world_heading_radians(&self) -> f32 {
        self.heading
    }
}

#[derive(Component)]
pub struct ShipSceneRoot;

#[derive(Component, Clone, Copy)]
pub struct ShipAppearance {
    #[allow(dead_code)]
    pub class: ShipClass,
}

#[derive(Component)]
pub struct ShipSceneInstance;

#[derive(Component, Clone, Copy)]
pub struct ShipFootprint {
    pub translation: Vec3,
    pub scale: Vec3,
}

#[derive(Component, Clone, Copy)]
pub struct ShipScenePlacement {
    pub translation: Vec3,
    pub scale: Vec3,
}

#[derive(Component, Clone, Copy)]
pub struct ShipModelBounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Component)]
pub struct ProjectedShip {
    pub ship_id: u64,
    pub lat: f64,
    pub lon: f64,
    pub cog: Option<f64>,
    pub sog: Option<f64>,
}

#[derive(Component)]
pub struct PhysicalShip {
    pub ship_id: u64,
    pub projected_entity: Entity,
    pub sync_class_from_db: bool,
    pub roll_phase_offset: f32,
    pub pitch_phase_offset: f32,
    pub roll_amplitude_radians: f32,
    pub pitch_amplitude_radians: f32,
}

#[derive(Component)]
pub struct ShipRouteRoot {
    pub ship_id: u64,
    pub projected_entity: Entity,
}

#[derive(Default, Resource)]
pub struct SelectedShipRoute(pub Option<Entity>);

pub fn spawn_projected_ship_pair(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projection: &TileWorldProjection,
    water_height: f32,
    map_root: &MapRoot,
    class: ShipClass,
    ship_id: u64,
    name: &str,
    lat: f64,
    lon: f64,
    cog: Option<f64>,
    sog: Option<f64>,
) -> (Entity, Entity) {
    let target_heading = target_heading_from_cog(cog);
    let projected_ship = Ship {
        lat,
        lon,
        cog,
        sog,
        heading: target_heading,
    };

    let projected_entity = spawn_projected_ship_entity(
        commands,
        projection,
        water_height,
        map_root,
        name,
        ship_id,
        projected_ship,
    );

    let physical_entity = spawn_ship_scene_entity(
        commands,
        asset_server,
        projection,
        water_height,
        map_root,
        name,
        class,
        projected_ship,
        Some(new_physical_ship(ship_id, projected_entity)),
    );

    (projected_entity, physical_entity)
}

pub fn smooth_physical_ships(
    time: Res<Time>,
    projected_ships: Query<&ProjectedShip>,
    mut physical_ships: Query<(&PhysicalShip, &mut Ship), Without<ProjectedShip>>,
) {
    let heading_alpha = smoothing_alpha_f32(SHIP_HEADING_SMOOTHING_RATE, time.delta_secs());

    for (physical_ship, mut ship) in &mut physical_ships {
        let Ok(projected_ship) = projected_ships.get(physical_ship.projected_entity) else {
            continue;
        };

        ship.lat = projected_ship.lat;
        ship.lon = projected_ship.lon;
        ship.sog = projected_ship.sog;
        ship.cog = projected_ship.cog;
        ship.heading = lerp_angle(ship.heading, projected_ship.world_heading_radians(), heading_alpha);
    }
}

pub fn sync_ships_to_map(
    projection: Res<TileWorldProjection>,
    water_settings: Res<WaterSettings>,
    mut ships: Query<(&Ship, &mut Transform), With<ShipSceneRoot>>,
) {
    for (ship, mut transform) in &mut ships {
        let mut ship_position = projection.lat_lon_to_world(ship.lat, ship.lon);
        ship_position.y = water_settings.height;

        transform.translation = ship_position;
        transform.rotation = Quat::from_rotation_y(ship.world_heading_radians());
    }
}

pub fn sync_ship_footprints_from_db(
    mut commands: Commands,
    connection: Option<Res<StdbConn>>,
    physical_ships: Query<(Entity, &PhysicalShip, Option<&ShipFootprint>)>,
) {
    let Some(connection) = connection else {
        return;
    };

    for (entity, physical_ship, current_footprint) in &physical_ships {
        let next_footprint = connection
            .db()
            .ship()
            .mmsi()
            .find(&physical_ship.ship_id)
            .as_ref()
            .and_then(ship_footprint_from_record);

        match (current_footprint, next_footprint) {
            (Some(current), Some(next)) if current.translation == next.translation && current.scale == next.scale => {}
            (_, Some(next)) => {
                commands.queue_silenced(move |world: &mut World| {
                    let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
                        return;
                    };
                    entity_mut.insert(next);
                });
            }
            (Some(_), None) => {
                commands.queue_silenced(move |world: &mut World| {
                    let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
                        return;
                    };
                    entity_mut.remove::<ShipFootprint>();
                });
            }
            (None, None) => {}
        }
    }
}

pub fn sync_ship_scene_placements(
    root_ships: Query<(&ShipAppearance, Option<&ShipFootprint>, &Children), With<ShipSceneRoot>>,
    mut scene_instances: Query<
        (&mut ShipScenePlacement, Option<&ShipModelBounds>),
        With<ShipSceneInstance>,
    >,
) {
    for (appearance, footprint, children) in &root_ships {
        let class_spec = appearance.class.spec();
        let fallback = ShipScenePlacement {
            translation: class_spec.model_translation,
            scale: class_spec.model_scale,
        };

        for child in children.iter() {
            let Ok((mut placement, model_bounds)) = scene_instances.get_mut(child) else {
                continue;
            };

            *placement = footprint
                .zip(model_bounds)
                .and_then(|(footprint, model_bounds)| {
                    fit_ship_scene_to_footprint(class_spec, *footprint, *model_bounds)
                })
                .unwrap_or(fallback);
        }
    }
}

pub fn sync_physical_ship_classes(
    mut commands: Commands,
    connection: Option<Res<StdbConn>>,
    physical_ships: Query<
        (Entity, &PhysicalShip, &ShipAppearance, Option<&Children>, &Name),
        With<ShipSceneRoot>,
    >,
    ship_scene_instances: Query<(), With<ShipSceneInstance>>,
) {
    let Some(connection) = connection else {
        return;
    };

    for (entity, physical_ship, appearance, children, name) in &physical_ships {
        if !physical_ship.sync_class_from_db {
            continue;
        }

        let desired_class = connection
            .db()
            .ship()
            .mmsi()
            .find(&physical_ship.ship_id)
            .as_ref()
            .map(|ship| ShipClass::from_major_ais_type(ship.major_ship_type.as_ref()))
            .unwrap_or(ShipClass::Default);

        if appearance.class == desired_class {
            continue;
        }

        let scene_instance_children = children
            .map(|children| {
                children
                    .iter()
                    .filter(|child| ship_scene_instances.get(*child).is_ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let ship_name = name.as_str().to_owned();

        commands.queue_silenced(move |world: &mut World| {
            let Ok(mut parent_entity) = world.get_entity_mut(entity) else {
                return;
            };

            parent_entity.insert(ShipAppearance {
                class: desired_class,
            });

            for child in scene_instance_children {
                if let Ok(entity_mut) = world.get_entity_mut(child) {
                    entity_mut.despawn();
                }
            }

            spawn_ship_scene_instance_entity_in_world(world, entity, &ship_name, desired_class);
        });
    }
}

pub fn select_ship_on_click(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    egui_wants_input: Res<EguiWantsInput>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    ships: Query<(&Ship, &Transform, Option<&PhysicalShip>, &Name), With<ShipSceneRoot>>,
    connection: Option<Res<StdbConn>>,
    projection: Res<TileWorldProjection>,
    map_root: Res<MapRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut selected_route: ResMut<SelectedShipRoute>,
    mut ship_info: ResMut<ShipInfoOverlay>,
) {
    if !buttons.just_pressed(MouseButton::Left) || egui_wants_input.is_pointer_over_area() {
        return;
    }

    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    let Ok(ray) = camera.0.viewport_to_world(camera.1, cursor_position) else {
        return;
    };

    let ray_origin = ray.origin;
    let ray_direction = ray.direction.as_vec3();
    let mut closest_hit = None;

    for (ship, transform, physical_ship, name) in &ships {
        let distance = ray_sphere_hit_distance(
            ray_origin,
            ray_direction,
            transform.translation,
            SHIP_PICK_RADIUS,
        );

        let Some(distance) = distance else {
            continue;
        };

        if closest_hit.is_none_or(|(_, best_distance)| distance < best_distance) {
            closest_hit = Some(((ship, physical_ship, name), distance));
        }
    }

    let Some(((ship, physical_ship, name), _)) = closest_hit else {
        clear_selected_ship(&mut ship_info);
        despawn_selected_ship_route(&mut commands, &mut selected_route);
        return;
    };

    despawn_selected_ship_route(&mut commands, &mut selected_route);

    ship_info.ship_id = physical_ship.map(|physical_ship| physical_ship.ship_id);
    ship_info.name = name.as_str().to_owned();
    ship_info.call_sign = None;
    ship_info.destination = None;
    ship_info.ship_type = None;
    ship_info.dimension_a = None;
    ship_info.dimension_b = None;
    ship_info.dimension_c = None;
    ship_info.dimension_d = None;
    ship_info.course_over_ground = ship.cog;
    ship_info.speed_over_ground = ship.sog;
    ship_info.latitude = ship.lat;
    ship_info.longitude = ship.lon;

    if let Some(physical_ship) = physical_ship {
        let ship_row = connection
            .as_deref()
            .and_then(|connection| connection.db().ship().mmsi().find(&physical_ship.ship_id));

        info!(
            ship_id = physical_ship.ship_id,
            ship_lookup = ?ship_row,
            "looked up ship metadata for selected ship"
        );

        if let Some(ship_row) = ship_row {
            ship_info.name = ship_row.name;
            ship_info.call_sign = ship_row.call_sign;
            ship_info.destination = ship_row.destination;
            ship_info.ship_type = ship_row.major_ship_type;
            ship_info.dimension_a = ship_row.dimension_a;
            ship_info.dimension_b = ship_row.dimension_b;
            ship_info.dimension_c = ship_row.dimension_c;
            ship_info.dimension_d = ship_row.dimension_d;
        }

        if let Some(connection) = connection.as_deref() {
            spawn_selected_ship_route(
                &mut commands,
                &projection,
                &map_root,
                &mut meshes,
                &mut materials,
                &mut selected_route,
                connection,
                physical_ship,
            );
        }
    }
}

pub fn sync_selected_ship_info(
    mut commands: Commands,
    physical_ships: Query<&PhysicalShip>,
    projected_ships: Query<&ProjectedShip>,
    connection: Option<Res<StdbConn>>,
    mut selected_route: ResMut<SelectedShipRoute>,
    mut ship_info: ResMut<ShipInfoOverlay>,
) {
    let Some(ship_id) = ship_info.ship_id else {
        return;
    };

    let Some(physical_ship) = physical_ships.iter().find(|ship| ship.ship_id == ship_id) else {
        clear_selected_ship(&mut ship_info);
        despawn_selected_ship_route(&mut commands, &mut selected_route);
        return;
    };

    let Ok(projected_ship) = projected_ships.get(physical_ship.projected_entity) else {
        clear_selected_ship(&mut ship_info);
        despawn_selected_ship_route(&mut commands, &mut selected_route);
        return;
    };

    ship_info.latitude = projected_ship.lat;
    ship_info.longitude = projected_ship.lon;
    ship_info.course_over_ground = projected_ship.cog;
    ship_info.speed_over_ground = projected_ship.sog;

    if let Some(ship_row) = connection
        .as_deref()
        .and_then(|connection| connection.db().ship().mmsi().find(&ship_id))
    {
        ship_info.name = ship_row.name;
        ship_info.call_sign = ship_row.call_sign;
        ship_info.destination = ship_row.destination;
        ship_info.ship_type = ship_row.major_ship_type;
        ship_info.dimension_a = ship_row.dimension_a;
        ship_info.dimension_b = ship_row.dimension_b;
        ship_info.dimension_c = ship_row.dimension_c;
        ship_info.dimension_d = ship_row.dimension_d;
    }
}

pub fn despawn_selected_route_when_projection_missing(
    mut commands: Commands,
    projected_ships: Query<(), With<ProjectedShip>>,
    route_roots: Query<(Entity, &ShipRouteRoot)>,
    mut selected_route: ResMut<SelectedShipRoute>,
) {
    for (entity, route_root) in &route_roots {
        if projected_ships.get(route_root.projected_entity).is_ok() {
            continue;
        }

        info!(ship_id = route_root.ship_id, "despawning selected ship route with missing projection");

        if selected_route.0 == Some(entity) {
            selected_route.0 = None;
        }

        commands.entity(entity).despawn();
    }
}

fn spawn_selected_ship_route(
    commands: &mut Commands,
    projection: &TileWorldProjection,
    map_root: &MapRoot,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    selected_route: &mut SelectedShipRoute,
    connection: &StdbConn,
    physical_ship: &PhysicalShip,
) {
    let location_reports = ship_location_reports(connection, physical_ship.ship_id);
    let Some(route_mesh) = create_route(projection, &location_reports) else {
        return;
    };

    let route_root = commands
        .spawn((
            Name::new(format!("Ship Route {}", physical_ship.ship_id)),
            ShipRouteRoot {
                ship_id: physical_ship.ship_id,
                projected_entity: physical_ship.projected_entity,
            },
            ChildOf(map_root.0),
            Transform::default(),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    let mesh = meshes.add(route_mesh);
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.91, 0.73, 0.24, 0.75),
        emissive: LinearRgba::rgb(0.25, 0.19, 0.05),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        Name::new(format!("Ship Route Mesh {}", physical_ship.ship_id)),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        ChildOf(route_root),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
        InheritedVisibility::default(),
    ));

    selected_route.0 = Some(route_root);
}

fn ship_location_reports(connection: &StdbConn, ship_id: u64) -> Vec<LocationReport> {
    let mut reports = connection
        .db()
        .location_report()
        .iter()
        .filter(|row| row.ship_mmsi == ship_id)
        .collect::<Vec<_>>();

    reports.sort_by_key(|row| row.timestamp);
    reports
}

fn create_route(projection: &TileWorldProjection, location_reports: &[LocationReport]) -> Option<Mesh> {
    let mut points = Vec::with_capacity(location_reports.len());

    for report in location_reports {
        let mut position = projection.lat_lon_to_world(report.lat, report.lon);
        position.y = ROUTE_HEIGHT;

        if points
            .last()
            .is_some_and(|last: &Vec3| last.distance_squared(position) < 0.0001)
        {
            continue;
        }

        points.push(position);
    }

    if points.len() < 2 {
        return None;
    }

    let mut vertices = Vec::with_capacity(points.len() * 2);
    let mut normals = Vec::with_capacity(points.len() * 2);
    let mut uvs = Vec::with_capacity(points.len() * 2);
    let mut indices = Vec::with_capacity((points.len() - 1) * 6);
    let half_width = ROUTE_WIDTH * 0.5;

    for (index, point) in points.iter().enumerate() {
        let normal = route_normal(&points, index)?;
        let left = *point + Vec3::new(normal.x * half_width, 0.0, normal.y * half_width);
        let right = *point - Vec3::new(normal.x * half_width, 0.0, normal.y * half_width);
        let v = index as f32 / (points.len() - 1) as f32;

        vertices.push([left.x, left.y, left.z]);
        vertices.push([right.x, right.y, right.z]);
        normals.push([0.0, 1.0, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.0, v]);
        uvs.push([1.0, v]);

        if index + 1 < points.len() {
            let base = (index as u32) * 2;
            indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

fn route_normal(points: &[Vec3], index: usize) -> Option<Vec2> {
    let prev = index
        .checked_sub(1)
        .and_then(|prev| route_direction(points[prev], points[index]));
    let next = points
        .get(index + 1)
        .and_then(|next| route_direction(points[index], *next));

    let perpendicular = |direction: Vec2| Vec2::new(-direction.y, direction.x);

    match (prev, next) {
        (Some(prev), Some(next)) => {
            let blended = perpendicular(prev) + perpendicular(next);
            if blended.length_squared() > 0.0 {
                Some(blended.normalize())
            } else {
                Some(perpendicular(next))
            }
        }
        (Some(prev), None) => Some(perpendicular(prev)),
        (None, Some(next)) => Some(perpendicular(next)),
        (None, None) => None,
    }
}

fn route_direction(from: Vec3, to: Vec3) -> Option<Vec2> {
    let delta = Vec2::new((to.x - from.x) as f32, (to.z - from.z) as f32);
    (delta.length_squared() > 0.0).then_some(delta.normalize())
}

fn clear_selected_ship(ship_info: &mut ShipInfoOverlay) {
    ship_info.ship_id = None;
    ship_info.name.clear();
    ship_info.call_sign = None;
    ship_info.destination = None;
    ship_info.ship_type = None;
    ship_info.dimension_a = None;
    ship_info.dimension_b = None;
    ship_info.dimension_c = None;
    ship_info.dimension_d = None;
    ship_info.course_over_ground = None;
    ship_info.speed_over_ground = None;
    ship_info.latitude = 0.0;
    ship_info.longitude = 0.0;
}

fn despawn_selected_ship_route(commands: &mut Commands, selected_route: &mut SelectedShipRoute) {
    if let Some(route_root) = selected_route.0.take() {
        commands.entity(route_root).despawn();
    }
}

pub fn configure_spawned_ship_scene(
    trigger: On<SceneInstanceReady>,
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    scene_spawner: Res<SceneSpawner>,
    ship_scene_instances: Query<(), With<ShipSceneInstance>>,
    mesh_entities: Query<(&Mesh3d, &GlobalTransform)>,
    transforms: Query<&GlobalTransform>,
) {
    let root = trigger.event().entity;

    if ship_scene_instances.get(root).is_err() {
        return;
    }

    let Ok(root_transform) = transforms.get(root) else {
        return;
    };

    let instance_from_world = root_transform.affine().inverse();
    let mut merged_min = Vec3::splat(f32::INFINITY);
    let mut merged_max = Vec3::splat(f32::NEG_INFINITY);

    for entity in scene_spawner.iter_instance_entities(trigger.event().instance_id) {
        let Ok((mesh_handle, mesh_transform)) = mesh_entities.get(entity) else {
            continue;
        };

        commands.entity(entity).insert(NoFrustumCulling);

        let Some(mesh) = meshes.get(mesh_handle) else {
            continue;
        };
        let Some(aabb) = mesh.compute_aabb() else {
            continue;
        };

        let mesh_from_instance = instance_from_world * mesh_transform.affine();

        for corner in aabb_corners(aabb) {
            let point = mesh_from_instance.transform_point3(corner);
            merged_min = merged_min.min(point);
            merged_max = merged_max.max(point);
        }
    }

    if merged_min.is_finite() && merged_max.is_finite() {
        commands.entity(root).insert(ShipModelBounds {
            min: merged_min,
            max: merged_max,
        });
    }
}

fn new_physical_ship(ship_id: u64, projected_entity: Entity) -> PhysicalShip {
    PhysicalShip {
        ship_id,
        projected_entity,
        sync_class_from_db: true,
        roll_phase_offset: (ship_id as f32 * 0.73).rem_euclid(std::f32::consts::TAU),
        pitch_phase_offset: (ship_id as f32 * 1.13).rem_euclid(std::f32::consts::TAU),
        roll_amplitude_radians: 5.0_f32.to_radians(),
        pitch_amplitude_radians: 2.5_f32.to_radians(),
    }
}

fn spawn_projected_ship_entity(
    commands: &mut Commands,
    projection: &TileWorldProjection,
    water_height: f32,
    map_root: &MapRoot,
    name: &str,
    ship_id: u64,
    ship: Ship,
) -> Entity {
    let mut ship_position = projection.lat_lon_to_world(ship.lat, ship.lon);
    ship_position.y = water_height;

    commands
        .spawn((
            Name::new(format!("{name} Projection")),
            ProjectedShip {
                ship_id,
                lat: ship.lat,
                lon: ship.lon,
                cog: ship.cog,
                sog: ship.sog,
            },
            ChildOf(map_root.0),
            Visibility::Hidden,
            InheritedVisibility::default(),
            GlobalTransform::default(),
            Transform::from_translation(ship_position),
        ))
        .id()
}

pub(crate) fn spawn_ship_scene_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projection: &TileWorldProjection,
    water_height: f32,  
    map_root: &MapRoot,
    name: &str,
    class: ShipClass,
    ship: Ship,
    physical_ship: Option<PhysicalShip>,
) -> Entity {
    let mut ship_position = projection.lat_lon_to_world(ship.lat, ship.lon);
    ship_position.y = water_height;
    let ship_heading = ship.world_heading_radians();

    let root_entity = commands
        .spawn((
        Name::new(name.to_owned()),
        ship,
        ShipAppearance { class },
        ShipSceneRoot,
        ChildOf(map_root.0),
        Visibility::default(),
        InheritedVisibility::default(),
        GlobalTransform::default(),
        Transform::from_translation(ship_position)
            .with_rotation(Quat::from_rotation_y(ship_heading)),
        ))
        .id();

    spawn_ship_scene_instance_entity(commands, asset_server, root_entity, name, class);

    if let Some(physical_ship) = physical_ship {
        commands.entity(root_entity).insert(physical_ship);
    }

    root_entity
}

fn spawn_ship_scene_instance_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    parent_entity: Entity,
    name: &str,
    class: ShipClass,
) -> Entity {
    let class_spec = class.spec();

    let scene_handle: Handle<Scene> = match Path::new(class_spec.scene_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("obj") => asset_server.load(class_spec.scene_path),
        _ => asset_server.load(GltfAssetLabel::Scene(0).from_asset(class_spec.scene_path)),
    };
    let scene_root = SceneRoot(scene_handle);

    commands
        .spawn((
            Name::new(format!("{name} Model")),
            ShipSceneInstance,
            ShipScenePlacement {
                translation: class_spec.model_translation,
                scale: class_spec.model_scale,
            },
            ChildOf(parent_entity),
            scene_root,
            Transform::from_translation(class_spec.model_translation)
                .with_rotation(class_spec.model_rotation)
                .with_scale(class_spec.model_scale),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id()
}

fn spawn_ship_scene_instance_entity_in_world(
    world: &mut World,
    parent_entity: Entity,
    name: &str,
    class: ShipClass,
) -> Entity {
    let class_spec = class.spec();
    let asset_server = world.resource::<AssetServer>();

    let scene_handle: Handle<Scene> = match Path::new(class_spec.scene_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("obj") => asset_server.load(class_spec.scene_path),
        _ => asset_server.load(GltfAssetLabel::Scene(0).from_asset(class_spec.scene_path)),
    };
    let scene_root = SceneRoot(scene_handle);

    world
        .spawn((
            Name::new(format!("{name} Model")),
            ShipSceneInstance,
            ShipScenePlacement {
                translation: class_spec.model_translation,
                scale: class_spec.model_scale,
            },
            ChildOf(parent_entity),
            scene_root,
            Transform::from_translation(class_spec.model_translation)
                .with_rotation(class_spec.model_rotation)
                .with_scale(class_spec.model_scale),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id()
}

fn smoothing_alpha_f32(rate: f32, delta_seconds: f32) -> f32 {
    1.0 - (-rate * delta_seconds).exp()
}

fn lerp_angle(current: f32, target: f32, alpha: f32) -> f32 {
    let delta = (target - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    current + delta * alpha
}

impl ProjectedShip {
    fn world_heading_radians(&self) -> f32 {
        target_heading_from_cog(self.cog)
    }
}

fn target_heading_from_cog(cog: Option<f64>) -> f32 {
    cog.filter(|cog| cog.is_finite())
        .map(|cog| -((cog.rem_euclid(360.0)) as f32).to_radians())
        .unwrap_or(SHIP_HEADING)
}


fn ship_footprint_from_record(ship: &ShipRecord) -> Option<ShipFootprint> {
    let fore = f32::from(ship.dimension_a?);
    let aft = f32::from(ship.dimension_b?);
    let port = f32::from(ship.dimension_c?);
    let starboard = f32::from(ship.dimension_d?);

    let length = fore + aft;
    let width = port + starboard;

    if length <= 0.0 || width <= 0.0 {
        return None;
    }

    // AIS dimensions are offsets from the navigation reference point.
    let translation = Vec3::new(
        (starboard - port) * 0.5,
        SHIP_FOOTPRINT_Y_OFFSET,
        (aft - fore) * 0.5,
    );
    let scale = Vec3::new(width, SHIP_FOOTPRINT_HEIGHT, length);

    Some(ShipFootprint { translation, scale })
}

fn fit_ship_scene_to_footprint(
    class_spec: crate::ship_class::ShipClassSpec,
    footprint: ShipFootprint,
    model_bounds: ShipModelBounds,
) -> Option<ShipScenePlacement> {
    let model_size = model_bounds.max - model_bounds.min;
    let model_center = (model_bounds.min + model_bounds.max) * 0.5;
    let target_size = footprint.scale;

    if target_size.x <= 0.0 || target_size.z <= 0.0 {
        return None;
    }

    let width_axis = dominant_model_axis(class_spec.model_rotation, Vec3::X)?;
    let length_axis = dominant_model_axis(class_spec.model_rotation, Vec3::Z)?;

    if width_axis == length_axis {
        return None;
    }

    let mut scale = class_spec.model_scale;
    let width_extent = axis_component(model_size, width_axis);
    let length_extent = axis_component(model_size, length_axis);

    if width_extent <= 0.0 || length_extent <= 0.0 {
        return None;
    }

    set_axis_component(&mut scale, width_axis, target_size.x / width_extent);
    set_axis_component(&mut scale, length_axis, target_size.z / length_extent);

    let horizontal_scale = match (width_axis, length_axis) {
        (0, 1) | (1, 0) => scale.z,
        (0, 2) | (2, 0) => (scale.x + scale.z) * 0.5,
        (1, 2) | (2, 1) => scale.x,
        _ => scale.y,
    };

    for axis in 0..3 {
        if axis != width_axis && axis != length_axis {
            set_axis_component(&mut scale, axis, horizontal_scale);
        }
    }

    let anchor = Vec3::new(model_center.x, model_center.y, model_center.z);
    let rotated_anchor = class_spec.model_rotation * (anchor * scale);
    let translation = Vec3::new(
        class_spec.model_translation.x + footprint.translation.x - rotated_anchor.x,
        class_spec.model_translation.y,
        class_spec.model_translation.z + footprint.translation.z - rotated_anchor.z,
    );

    Some(ShipScenePlacement { translation, scale })
}

fn dominant_model_axis(rotation: Quat, ship_axis: Vec3) -> Option<usize> {
    let candidates = [Vec3::X, Vec3::Y, Vec3::Z];
    let mut best_axis = None;
    let mut best_score = 0.0_f32;

    for (axis, basis) in candidates.into_iter().enumerate() {
        let score = (rotation * basis).dot(ship_axis).abs();
        if score > best_score {
            best_score = score;
            best_axis = Some(axis);
        }
    }

    (best_score > 0.5).then_some(best_axis?).or(best_axis)
}

fn axis_component(vector: Vec3, axis: usize) -> f32 {
    match axis {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

fn set_axis_component(vector: &mut Vec3, axis: usize, value: f32) {
    match axis {
        0 => vector.x = value,
        1 => vector.y = value,
        _ => vector.z = value,
    }
}

fn aabb_corners(aabb: bevy::camera::primitives::Aabb) -> [Vec3; 8] {
    let min = aabb.min();
    let max = aabb.max();

    [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(max.x, max.y, max.z),
    ]
}

fn ray_sphere_hit_distance(
    ray_origin: Vec3,
    ray_direction: Vec3,
    sphere_center: Vec3,
    sphere_radius: f32,
) -> Option<f32> {
    let to_center = ray_origin - sphere_center;
    let a = ray_direction.length_squared();
    let b = 2.0 * ray_direction.dot(to_center);
    let c = to_center.length_squared() - sphere_radius * sphere_radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return None;
    }

    let sqrt_discriminant = discriminant.sqrt();
    let near = (-b - sqrt_discriminant) / (2.0 * a);
    let far = (-b + sqrt_discriminant) / (2.0 * a);

    if near >= 0.0 {
        Some(near)
    } else if far >= 0.0 {
        Some(far)
    } else {
        None
    }
}
