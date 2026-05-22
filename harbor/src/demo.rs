use bevy::prelude::*;
use bevy_water::WaterSettings;

use crate::map::{MapRoot, TileWorldProjection};
use crate::ship::{PhysicalShip, Ship, spawn_ship_scene_entity};
use crate::ship_class::ShipClass;

const SHIP_LATITUDE: f64 = 51.9060;
const SHIP_LONGITUDE: f64 = 4.4844;
const DEMO_SHIP_OFFSET_SOUTH_METERS: f64 = 350.0;
const DEMO_SHIP_OFFSET_WEST_METERS: f64 = 800.0;
const DEMO_SHIP_SPACING_METERS: f64 = 100.0;

#[derive(Component)]
pub struct DemoShip;

#[derive(Resource, Default)]
pub struct DemoShipsVisible(pub bool);

pub fn spawn_demo_ships(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    projection: Res<TileWorldProjection>,
    map_root: Res<MapRoot>,
    water_settings: Res<WaterSettings>,
) {
    commands.insert_resource(DemoShipsVisible(false));

    let demo_classes = [
        ShipClass::Default,
        ShipClass::Tug,
        ShipClass::Speedboat,
        ShipClass::Sailboat,
        ShipClass::Tanker,
        ShipClass::Military,
        ShipClass::Passenger,
        ShipClass::LawEnforcement,
        ShipClass::DredgingOrUnderwaterOps,
    ];
    let base_lat = SHIP_LATITUDE - latitude_offset_for_meters(DEMO_SHIP_OFFSET_SOUTH_METERS);
    let base_lon =
        SHIP_LONGITUDE - longitude_offset_for_meters(DEMO_SHIP_OFFSET_WEST_METERS, base_lat);

    for (index, class) in demo_classes.into_iter().enumerate() {
        let ship_id = index as u64;
        let east_offset_meters = index as f64 * DEMO_SHIP_SPACING_METERS;
        let lon = base_lon + longitude_offset_for_meters(east_offset_meters, base_lat);

        let root = spawn_ship_scene_entity(
            &mut commands,
            &asset_server,
            &projection,
            water_settings.height,
            &map_root,
            demo_ship_name(class),
            class,
            Ship {
                lat: base_lat,
                lon,
                cog: None,
                sog: None,
                heading: 0.0,
            },
            Some(new_demo_physical_ship(ship_id, Entity::PLACEHOLDER)),
        );
        commands
            .entity(root)
            .insert((DemoShip, Visibility::Hidden));
    }
}

pub fn toggle_demo_ships(
    input: Res<ButtonInput<KeyCode>>,
    mut demo_visible: ResMut<DemoShipsVisible>,
    mut demo_ships: Query<&mut Visibility, With<DemoShip>>,
) {
    if !input.just_pressed(KeyCode::KeyP) {
        return;
    }

    demo_visible.0 = !demo_visible.0;
    let visibility = if demo_visible.0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut ship_visibility in &mut demo_ships {
        *ship_visibility = visibility;
    }
}

fn new_demo_physical_ship(ship_id: u64, projected_entity: Entity) -> PhysicalShip {
    PhysicalShip {
        ship_id,
        projected_entity,
        sync_class_from_db: false,
        roll_phase_offset: (ship_id as f32 * 0.73).rem_euclid(std::f32::consts::TAU),
        pitch_phase_offset: (ship_id as f32 * 1.13).rem_euclid(std::f32::consts::TAU),
        roll_amplitude_radians: 5.0_f32.to_radians(),
        pitch_amplitude_radians: 2.5_f32.to_radians(),
    }
}

fn demo_ship_name(class: ShipClass) -> &'static str {
    match class {
        ShipClass::Default => "Default Demo Ship",
        ShipClass::Tug => "Tug Demo Ship",
        ShipClass::Speedboat => "Speedboat Demo Ship",
        ShipClass::Sailboat => "Sailboat Demo Ship",
        ShipClass::Tanker => "Tanker Demo Ship",
        ShipClass::Military => "Military Demo Ship",
        ShipClass::Passenger => "Passenger Demo Ship",
        ShipClass::LawEnforcement => "Law Enforcement Demo Ship",
        ShipClass::DredgingOrUnderwaterOps => "Dredging Demo Ship",
    }
}

fn longitude_offset_for_meters(east_meters: f64, latitude: f64) -> f64 {
    let meters_per_degree = 111_320.0 * latitude.to_radians().cos().abs();
    if meters_per_degree <= f64::EPSILON {
        0.0
    } else {
        east_meters / meters_per_degree
    }
}

fn latitude_offset_for_meters(north_meters: f64) -> f64 {
    north_meters / 111_320.0
}
