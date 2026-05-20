use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::ecs::system::Commands;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy_panorbit_wasd_camera::PanOrbitCamera;

use crate::map::TileWorldProjection;
use crate::ui::ShipInfoOverlay;

const CAMERA_LATITUDE: f64 = 51.90189;
const CAMERA_LONGITUDE: f64 = 4.49171;
const CAMERA_HEIGHT_METERS: f32 = 100.0;
const CAMERA_LOOK_DISTANCE_METERS: f32 = 500.0;
const CAMERA_PRESET_COUNT: usize = 5;
const CAMERA_POSE_LOG_INTERVAL_SECS: f32 = 5.0;
const CAMERA_PRESET_POSITION_EPSILON: f32 = 0.5;
const CAMERA_PRESET_FOCUS_EPSILON: f32 = 0.5;
const CAMERA_FOLLOW_DISTANCE_METERS: f32 = 300.0;
const CAMERA_FOLLOW_HEIGHT_METERS: f32 = 140.0;
const CAMERA_FOLLOW_FOCUS_HEIGHT_METERS: f32 = 10.5;
const CAMERA_PRESET_1: CameraPreset = CameraPreset {
    position: Vec3::new(19911.994, 463.758, 12811.165),
    focus: Vec3::new(20657.500, 101.591, 12354.323),
};
const CAMERA_PRESET_2: CameraPreset = CameraPreset {
    position: Vec3::new(78719.781, 4119.730, 23509.740),
    focus: Vec3::new(87854.555, 100.000, 27452.068),
};
const CAMERA_PRESET_3: CameraPreset = CameraPreset {
    position: Vec3::new(116714.328, 7522.703, 28497.092),
    focus: Vec3::new(108762.227, 100.000, 28145.459),
};

#[derive(Clone, Copy)]
pub struct CameraPreset {
    pub position: Vec3,
    pub focus: Vec3,
}

#[derive(Resource)]
pub struct CameraPresets(pub [CameraPreset; CAMERA_PRESET_COUNT]);

#[derive(Clone, Copy, Resource)]
pub struct CameraTransition {
    pub active: bool,
    pub target_position: Vec3,
    pub target_focus: Vec3,
    pub mode: CameraTransitionMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraTransitionMode {
    Preset,
    AimSelectedShip,
    FollowSelectedShip,
}

#[derive(Resource)]
pub struct CameraPoseLogTimer(pub Timer);

/// set up a simple 3D camera
pub fn setup_camera(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    projection: Res<TileWorldProjection>,
) {
    let default_preset = default_camera_preset(*projection);

    commands.insert_resource(CameraPresets([
        CAMERA_PRESET_1,
        CAMERA_PRESET_2,
        CAMERA_PRESET_3,
        default_preset,
        default_preset,
    ]));
    commands.insert_resource(CameraTransition {
        active: false,
        target_position: default_preset.position,
        target_focus: default_preset.focus,
        mode: CameraTransitionMode::Preset,
    });
    commands.insert_resource(CameraPoseLogTimer(Timer::from_seconds(
        CAMERA_POSE_LOG_INTERVAL_SECS,
        TimerMode::Repeating,
    )));

    make_camera(&mut commands, &asset_server, *projection);
}

/// Create a simple 3D camera
pub fn make_camera<'a>(
    commands: &'a mut Commands,
    asset_server: &AssetServer,
    projection: TileWorldProjection,
) -> EntityCommands<'a> {
    let preset = default_camera_preset(projection);
    let camera_position = preset.position;
    let focus = preset.focus;

    // camera
    let mut cam = commands.spawn((
        // Camera3d::default(),
        // Transform::from_xyz(-20.0, WATER_HEIGHT + 5.0, 20.0)
        //     .looking_at(Vec3::new(0.0, WATER_HEIGHT, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server
                .load("environment_maps/table_mountain_2_puresky_4k_diffuse.ktx2"),
            specular_map: asset_server
                .load("environment_maps/table_mountain_2_puresky_4k_specular.ktx2"),
            intensity: 1.0,
            ..default()
        },
        DistanceFog {
            color: Color::srgba(0.1, 0.2, 0.4, 1.0),
            //directional_light_color: Color::srgba(1.0, 0.95, 0.75, 0.5),
            //directional_light_exponent: 30.0,
            falloff: FogFalloff::from_visibility_colors(
                15000.0, // distance in world units up to which objects retain visibility (>= 5% contrast)
                Color::srgb(0.35, 0.5, 0.66), // atmospheric extinction color (after light is lost due to absorption by atmospheric particles)
                Color::srgb(0.8, 0.844, 1.0), // atmospheric inscattering color (light gained due to scattering from the sun)
            ),
            ..default()
        },
    ));

    // /data/v3/13/4198/2708.pbf

    // focus: Vec3::new(4198.0, 0.0, 2708.0),
    cam.insert((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(focus, Vec3::Y),
    ));

    #[cfg(target_arch = "wasm32")]
    cam.insert(Msaa::Off);

    cam.insert(PanOrbitCamera {
        focus,
        target_focus: focus,
        ..default()
    });

    {
        use bevy::core_pipeline::Skybox;

        cam.insert(Skybox {
            image: asset_server.load("environment_maps/table_mountain_2_puresky_4k_cubemap.ktx2"),
            brightness: 1000.0,
            ..default()
        });
        info!("Skybox enabled (feature `atmosphere` not enabled)");
    }

    // This will write the depth buffer to a texture that you can use in the main pass
    cam.insert(DepthPrepass);
    // This is just to keep the compiler happy when not using `depth_prepass` feature.
    cam.insert(Name::new("Camera"));

    info!("Move camera around by using WASD for lateral movement");
    info!("Use Left Shift and Spacebar for vertical movement");
    info!("Use the mouse to look around");
    info!("Press Esc to hide or show the mouse cursor");

    cam
}

pub fn activate_camera_presets(
    input: Res<ButtonInput<KeyCode>>,
    presets: Res<CameraPresets>,
    projection: Res<TileWorldProjection>,
    ship_info: Res<ShipInfoOverlay>,
    camera: Single<(&GlobalTransform, &mut PanOrbitCamera), With<Camera3d>>,
    mut transition: ResMut<CameraTransition>,
) {
    let target = if input.just_pressed(KeyCode::Digit1) {
        Some((presets.0[0], CameraTransitionMode::Preset))
    } else if input.just_pressed(KeyCode::Digit2) {
        Some((presets.0[1], CameraTransitionMode::Preset))
    } else if input.just_pressed(KeyCode::Digit3) {
        Some((presets.0[2], CameraTransitionMode::Preset))
    } else if input.just_pressed(KeyCode::Digit4) {
        aim_selected_ship_preset(&projection, &ship_info, camera.0.translation())
            .map(|preset| (preset, CameraTransitionMode::AimSelectedShip))
    } else if input.just_pressed(KeyCode::Digit5) {
        follow_selected_ship_preset(&projection, &ship_info)
            .map(|preset| (preset, CameraTransitionMode::FollowSelectedShip))
    } else {
        None
    };

    let Some((preset, mode)) = target else {
        return;
    };

    let (yaw, pitch, radius) = calculate_orbit_from_translation_and_focus(preset.position, preset.focus);
    let (_, mut pan_orbit) = camera.into_inner();

    pan_orbit.target_focus = preset.focus;
    pan_orbit.target_yaw = yaw;
    pan_orbit.target_pitch = pitch;
    pan_orbit.target_radius = radius;

    transition.active = true;
    transition.target_position = preset.position;
    transition.target_focus = preset.focus;
    transition.mode = mode;
}

pub fn update_follow_selected_ship_camera_target(
    projection: Res<TileWorldProjection>,
    ship_info: Res<ShipInfoOverlay>,
    camera: Single<&GlobalTransform, With<Camera3d>>,
    mut transition: ResMut<CameraTransition>,
) {
    if !transition.active {
        return;
    }

    let preset = match transition.mode {
        CameraTransitionMode::AimSelectedShip => {
            aim_selected_ship_preset(&projection, &ship_info, camera.translation())
        }
        CameraTransitionMode::FollowSelectedShip => follow_selected_ship_preset(&projection, &ship_info),
        CameraTransitionMode::Preset => None,
    };

    let Some(preset) = preset else {
        if matches!(
            transition.mode,
            CameraTransitionMode::AimSelectedShip | CameraTransitionMode::FollowSelectedShip
        ) {
            transition.active = false;
            transition.mode = CameraTransitionMode::Preset;
        }
        return;
    };

    transition.target_position = preset.position;
    transition.target_focus = preset.focus;
}

pub fn cancel_camera_preset_tween_on_manual_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut transition: ResMut<CameraTransition>,
) {
    if !transition.active {
        return;
    }

    let keyboard_movement = [
        KeyCode::KeyW,
        KeyCode::KeyA,
        KeyCode::KeyS,
        KeyCode::KeyD,
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::Space,
    ]
    .into_iter()
    .any(|key| keyboard.pressed(key));
    let mouse_movement = mouse_motion.read().next().is_some();

    if keyboard_movement || mouse_movement {
        transition.active = false;
        transition.mode = CameraTransitionMode::Preset;
    }
}

pub fn apply_camera_transition_targets(
    camera: Single<&mut PanOrbitCamera, With<Camera3d>>,
    transition: Res<CameraTransition>,
) {
    if !transition.active {
        return;
    }

    let (yaw, pitch, radius) =
        calculate_orbit_from_translation_and_focus(transition.target_position, transition.target_focus);
    let mut pan_orbit = camera.into_inner();
    pan_orbit.target_focus = transition.target_focus;
    pan_orbit.target_yaw = yaw;
    pan_orbit.target_pitch = pitch;
    pan_orbit.target_radius = radius;
}

pub fn finish_camera_preset_transition(
    mut transition: ResMut<CameraTransition>,
    camera: Single<(&GlobalTransform, &PanOrbitCamera), With<Camera3d>>,
) {
    if !transition.active || transition.mode != CameraTransitionMode::Preset {
        return;
    }

    let (global_transform, pan_orbit) = camera.into_inner();
    let position_done = global_transform.translation().distance(transition.target_position)
        <= CAMERA_PRESET_POSITION_EPSILON;
    let focus_done = pan_orbit.focus.distance(transition.target_focus) <= CAMERA_PRESET_FOCUS_EPSILON;

    if position_done && focus_done {
        transition.active = false;
    }
}

pub fn log_camera_pose_every_five_seconds(
    time: Res<Time>,
    mut timer: ResMut<CameraPoseLogTimer>,
    camera: Single<(&GlobalTransform, &Transform, &PanOrbitCamera), With<Camera3d>>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    let (global_transform, transform, pan_orbit) = camera.into_inner();
    let position = global_transform.translation();
    let forward = transform.forward().as_vec3();
    let up = transform.up().as_vec3();
    let focus = pan_orbit.focus;
    let rotation = transform.rotation;

    info!(
        position = ?position,
        focus = ?focus,
        forward = ?forward,
        up = ?up,
        rotation = ?rotation,
        preset = %format!(
            "CameraPreset {{ position: Vec3::new({:.3}, {:.3}, {:.3}), focus: Vec3::new({:.3}, {:.3}, {:.3}) }}",
            position.x,
            position.y,
            position.z,
            focus.x,
            focus.y,
            focus.z,
        ),
        "camera pose snapshot"
    );
}

fn default_camera_preset(projection: TileWorldProjection) -> CameraPreset {
    let ground_position = projection.lat_lon_to_world(CAMERA_LATITUDE, CAMERA_LONGITUDE);
    let position = ground_position + Vec3::Y * CAMERA_HEIGHT_METERS;
    let focus = position + Vec3::Z * CAMERA_LOOK_DISTANCE_METERS;

    CameraPreset { position, focus }
}

fn follow_selected_ship_preset(
    projection: &TileWorldProjection,
    ship_info: &ShipInfoOverlay,
) -> Option<CameraPreset> {
    ship_info.ship_id?;

    let mut focus = projection.lat_lon_to_world(ship_info.latitude, ship_info.longitude);
    focus.y = CAMERA_FOLLOW_FOCUS_HEIGHT_METERS;

    let heading = target_heading_from_cog(ship_info.course_over_ground);
    let forward = Quat::from_rotation_y(heading).mul_vec3(Vec3::Z);
    let behind = -forward.normalize_or_zero();
    let position = focus + behind * CAMERA_FOLLOW_DISTANCE_METERS + Vec3::Y * CAMERA_FOLLOW_HEIGHT_METERS;

    Some(CameraPreset { position, focus })
}

fn aim_selected_ship_preset(
    projection: &TileWorldProjection,
    ship_info: &ShipInfoOverlay,
    position: Vec3,
) -> Option<CameraPreset> {
    ship_info.ship_id?;

    let mut focus = projection.lat_lon_to_world(ship_info.latitude, ship_info.longitude);
    focus.y = CAMERA_FOLLOW_FOCUS_HEIGHT_METERS;

    Some(CameraPreset { position, focus })
}

fn calculate_orbit_from_translation_and_focus(translation: Vec3, focus: Vec3) -> (f32, f32, f32) {
    let comp_vec = translation - focus;
    let mut radius = comp_vec.length();
    if radius == 0.0 {
        radius = 0.05;
    }

    let yaw = comp_vec.x.atan2(comp_vec.z);
    let pitch = (comp_vec.y / radius).asin();
    (yaw, pitch, radius)
}

fn target_heading_from_cog(cog: Option<f64>) -> f32 {
    cog.filter(|cog| cog.is_finite())
        .map(|cog| -((cog.rem_euclid(360.0)) as f32).to_radians())
        .unwrap_or(0.0)
}
