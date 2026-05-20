use bevy::{
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder},
    prelude::*,
};

#[cfg(target_arch = "wasm32")]
const DIRECTIONAL_LIGHT_CASCADES: usize = 1;
#[cfg(not(target_arch = "wasm32"))]
const DIRECTIONAL_LIGHT_CASCADES: usize = 3;

pub fn light_start_system(mut cmd: Commands) {
    cmd.insert_resource(GlobalAmbientLight {
        color: Color::srgb_u8(210, 220, 240),
        brightness: 200.0, // Increased to lighten shadows
        affects_lightmapped_meshes: true,
    });

    cmd.spawn((
        DirectionalLight {
            illuminance: 40_000.0,
            shadows_enabled: true,
            ..Default::default()
        },
        Transform {
            translation: Vec3::new(0.0, 0.0, 0.0),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
        CascadeShadowConfig::from(CascadeShadowConfigBuilder {
            maximum_distance: 9000.0,
            minimum_distance: 0.2,
            num_cascades: DIRECTIONAL_LIGHT_CASCADES,
            first_cascade_far_bound: 200.0,
            ..Default::default()
        }),
    ));
}

const K: f32 = 2.;

pub fn animate_light_direction(
    time: Res<Time>,
    mut query: Query<&mut Transform, With<DirectionalLight>>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if input.pressed(KeyCode::KeyH) {
        for mut transform in &mut query {
            transform.rotate_y(time.delta_secs() * K);
        }
    }
    if input.pressed(KeyCode::KeyL) {
        for mut transform in &mut query {
            transform.rotate_y(-time.delta_secs() * K);
        }
    }
    if input.pressed(KeyCode::KeyJ) {
        for mut transform in &mut query {
            transform.rotate_x(time.delta_secs() * K);
        }
    }
    if input.pressed(KeyCode::KeyK) {
        for mut transform in &mut query {
            transform.rotate_x(-time.delta_secs() * K);
        }
    }
}
