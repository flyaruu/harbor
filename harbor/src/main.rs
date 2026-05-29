use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_obj::ObjPlugin;
use bevy_panorbit_wasd_camera::PanOrbitCameraPlugin;
use bevy_water::WaterSettings;
use std::time::Duration;

use crate::light::animate_light_direction;
use crate::perf::PerformancePlugin;
use crate::ship::SelectedShipRoute;
use crate::spacetime::SpacetimePlugin;
use crate::ui::{
    CurrentTimestamp, ShipInfoOverlay, TimestampBounds, TimestampPlayback, TimestampUi,
    timestamp_ui,
};

pub const WATER_LEVEL: f32 = -6.0;

mod camera;
mod demo;
mod light;
mod map;
mod module_bindings;
mod perf;
mod ship;
mod ship_class;
mod spacetime;
mod static_landmarks;
mod ui;
mod wave_rocking;

fn main() {
    configure_runtime();

    let mut app = App::new();
    map::configure_tile_asset_source(&mut app);

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Harbor".into(),
            present_mode: bevy::window::PresentMode::AutoVsync,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(ObjPlugin)
    .add_plugins(EguiPlugin::default())
    .add_plugins(PerformancePlugin)
    .add_plugins(SpacetimePlugin)
    .add_plugins(PanOrbitCameraPlugin)
    .insert_resource(ClearColor(Color::srgb(0.07, 0.09, 0.12)))
    .insert_resource(TimestampUi::default())
    .insert_resource(TimestampBounds::default())
    .insert_resource(TimestampPlayback::default())
    .insert_resource(ShipInfoOverlay::default())
    .insert_resource(ship::ShipLodConfig::default())
    .insert_resource(SelectedShipRoute::default())
    .insert_resource(CurrentTimestamp::default())
    .add_systems(
        Startup,
        (
            map::setup_map_tile_materials,
            map::setup_map_tiles,
            ship::setup_ship_lod_assets,
            static_landmarks::bridge::spawn_bridge,
            demo::spawn_demo_ships,
            camera::setup_camera,
            light::light_start_system,
        )
            .chain(),
    )
    .add_systems(EguiPrimaryContextPass, timestamp_ui)
    .add_systems(
        Update,
        ui::advance_timestamp_playback.run_if(bevy::time::common_conditions::on_timer(
            Duration::from_millis(100),
        )),
    )
    .add_systems(
        Update,
        (
            animate_light_direction,
            camera::log_camera_pose_every_five_seconds,
            demo::toggle_demo_ships,
            map::spawn_map_tile_batch,
            (
                ship::smooth_physical_ships,
                ship::sync_physical_ship_classes,
                ship::sync_ship_footprints_from_db,
                ship::sync_ship_scene_placements,
                ship::sync_ship_footprint_visuals,
                ship::sync_ships_to_map,
                ship::update_ship_lod_from_camera,
                ship::apply_ship_lod_visibility,
            )
                .chain(),
            wave_rocking::apply_wave_rocking,
            ship::select_ship_on_click,
            ship::sync_selected_ship_info,
            ship::despawn_selected_route_when_projection_missing,
            (
                camera::activate_camera_presets,
                camera::update_follow_selected_ship_camera_target,
                camera::cancel_camera_preset_tween_on_manual_input,
                camera::apply_camera_transition_targets,
                camera::finish_camera_preset_transition,
            )
                .chain()
                .after(ship::sync_selected_ship_info),
        ),
    )
    .add_observer(map::remap_map_tile_materials)
    .add_observer(ship::configure_spawned_ship_scene)
    .insert_resource(WaterSettings {
        height: WATER_LEVEL,
        amplitude: 3.5,
        spawn_tiles: None,
        edge_scale: 10.0,
        clarity: 0.1,
        base_color: Color::srgba(0.3, 0.3, 0.3, 1.0),
        deep_color: Color::srgba(0.15, 0.31, 0.44, 1.0),
        shallow_color: Color::srgba(0.35, 0.78, 0.51, 1.0),
        water_quality: bevy_water::WaterQuality::Medium,
        ..default()
    });

    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(
        Update,
        (
            map::update_desired_map_tiles,
            map::download_map_tile_batch,
            map::collect_completed_map_tile_downloads,
        )
            .chain()
            .before(map::spawn_map_tile_batch),
    );

    add_platform_plugins(&mut app);

    app.run();
}

#[cfg(target_arch = "wasm32")]
fn configure_runtime() {
    console_error_panic_hook::set_once();
}

#[cfg(not(target_arch = "wasm32"))]
fn configure_runtime() {}

#[cfg(not(target_arch = "wasm32"))]
fn add_platform_plugins(app: &mut App) {
    app.add_plugins(bevy_water::WaterPlugin);
}

#[cfg(target_arch = "wasm32")]
fn add_platform_plugins(_app: &mut App) {}
