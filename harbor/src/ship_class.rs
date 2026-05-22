use bevy::prelude::*;

use crate::module_bindings::MajorAisShipType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShipClass {
    Default,
    Tug,
    Speedboat,
    Sailboat,
    Tanker,
    Military,
    Passenger,
    LawEnforcement,
    DredgingOrUnderwaterOps,
}

#[derive(Debug, Clone, Copy)]
pub struct ShipClassSpec {
    pub scene_path: &'static str,
    pub model_translation: Vec3,
    pub model_rotation: Quat,
    pub model_scale: Vec3,
}

impl ShipClass {
    pub fn from_major_ais_type(ship_type: Option<&MajorAisShipType>) -> Self {
        let result = match ship_type {
            Some(
                MajorAisShipType::Tug
                | MajorAisShipType::Towing
                | MajorAisShipType::TowingLarge
                | MajorAisShipType::PortTender,
            ) => Self::Tug,
            Some(MajorAisShipType::HighSpeedCraft | MajorAisShipType::PleasureCraft) => {
                Self::Speedboat
            }
            Some(MajorAisShipType::Sailing) => Self::Sailboat,
            Some(MajorAisShipType::Tanker) => Self::Tanker,
            Some(MajorAisShipType::MilitaryOps) => Self::Military,
            Some(MajorAisShipType::Passenger) => Self::Passenger,
            Some(MajorAisShipType::LawEnforcement) => Self::LawEnforcement,
            Some(MajorAisShipType::DredgingOrUnderwaterOps) => Self::DredgingOrUnderwaterOps,
            _ => Self::Default,
        };
        // info!("Mapped AIS ship type {:?} to ship class {:?}", ship_type, result);
        result
    }

    pub fn spec(self) -> ShipClassSpec {
        match self {
            Self::Default => ShipClassSpec {
                scene_path: "models/container_ship.glb",
                model_translation: Vec3::new(0.0, 11.0, 0.0),
                model_rotation: Quat::IDENTITY,
                model_scale: Vec3::splat(150.0),
            },
            Self::Tug => ShipClassSpec {
                scene_path: "models/tugboat.glb",
                model_translation: Vec3::new(0.0, 0.5, 0.0),
                model_rotation: Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
                model_scale: Vec3::splat(10.0),
            },
            Self::Speedboat => ShipClassSpec {
                scene_path: "models/speedboat.glb",
                model_translation: Vec3::new(0.0, -1.0, 0.0),
                // Combine X and Y rotations by multiplying quaternions
                model_rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
                    * Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
                model_scale: Vec3::splat(0.10),
            },
            Self::Sailboat => ShipClassSpec {
                scene_path: "models/sail_boat.obj",
                model_translation: Vec3::new(0.0, 4.0, 0.0),
                model_rotation: Quat::IDENTITY,

                // model_rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                model_scale: Vec3::splat(0.2),
            },
            Self::Tanker => ShipClassSpec {
                scene_path: "models/oil_tanker.obj",
                model_translation: Vec3::new(0.0, -2.0, 0.0),
                model_rotation: Quat::IDENTITY,
                model_scale: Vec3::splat(0.5),
            },
            Self::Military => ShipClassSpec {
                scene_path: "models/frigate.obj",
                model_translation: Vec3::new(0.0, -2.0, 0.0),
                model_rotation: Quat::IDENTITY,
                model_scale: Vec3::splat(0.5),
            },
            Self::Passenger => ShipClassSpec {
                scene_path: "models/sydney_emerald-class_low_poly.glb",
                model_translation: Vec3::new(0.0, 3.0, 0.0),
                model_rotation: Quat::IDENTITY,
                model_scale: Vec3::splat(60.0),
            },
            Self::LawEnforcement => ShipClassSpec {
                scene_path: "models/police_boat.glb",
                model_translation: Vec3::new(0.0, 2.0, 0.0),
                model_rotation: Quat::IDENTITY,
                model_scale: Vec3::splat(3.0),
            },
            Self::DredgingOrUnderwaterOps => ShipClassSpec {
                scene_path: "models/dredge.glb",
                model_translation: Vec3::new(0.0, 3.0, 0.0),
                model_rotation: Quat::IDENTITY,
                model_scale: Vec3::splat(200.0),
            },
        }
    }
}
