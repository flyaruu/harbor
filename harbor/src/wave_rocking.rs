use bevy::prelude::*;

use crate::ship::{
    PhysicalShip, ShipAppearance, ShipSceneInstance, ShipScenePlacement, ShipSceneRoot,
};

const ROLL_FREQUENCY_HZ: f32 = 0.1;
const PITCH_FREQUENCY_HZ: f32 = 0.05;

pub fn apply_wave_rocking(
    time: Res<Time>,
    physical_ships: Query<(&PhysicalShip, &ShipAppearance, &Children), With<ShipSceneRoot>>,
    mut ship_scene_instances: Query<(&ShipScenePlacement, &mut Transform), With<ShipSceneInstance>>,
) {
    let elapsed = time.elapsed_secs();
    let roll_phase = std::f32::consts::TAU * ROLL_FREQUENCY_HZ * elapsed;
    let pitch_phase = std::f32::consts::TAU * PITCH_FREQUENCY_HZ * elapsed;

    for (physical_ship, appearance, children) in &physical_ships {
        let class_spec = appearance.class.spec();
        let roll = physical_ship.roll_amplitude_radians
            * (roll_phase + physical_ship.roll_phase_offset).sin();
        let pitch = physical_ship.pitch_amplitude_radians
            * (pitch_phase + physical_ship.pitch_phase_offset).sin();
        let rocking_rotation = Quat::from_rotation_z(roll) * Quat::from_rotation_x(pitch);

        for child in children.iter() {
            let Ok((placement, mut transform)) = ship_scene_instances.get_mut(child) else {
                continue;
            };

            transform.translation = placement.translation;
            transform.rotation = rocking_rotation * class_spec.model_rotation;
            transform.scale = placement.scale;
        }
    }
}
