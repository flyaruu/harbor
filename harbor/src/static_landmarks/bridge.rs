use bevy::prelude::*;

use crate::map::{MapRoot, TileWorldProjection};

const ERASMUS_BRIDGE_LATITUDE: f64 = 51.90953900421335;
const ERASMUS_BRIDGE_LONGITUDE: f64 = 4.48610066950323;
const ERASMUS_BRIDGE_HEIGHT: f32 = -8.639;
const ERASMUS_BRIDGE_YAW_DEGREES: f32 = -178.648;
const ERASMUS_BRIDGE_SCALE: Vec3 = Vec3::splat(2.677);

pub fn spawn_bridge(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    map_root: Res<MapRoot>,
    projection: Res<TileWorldProjection>,
) {
    let mut bridge_translation =
        projection.lat_lon_to_world(ERASMUS_BRIDGE_LATITUDE, ERASMUS_BRIDGE_LONGITUDE);
    bridge_translation.y = ERASMUS_BRIDGE_HEIGHT;

    commands.spawn((
        Name::new("Erasmus Bridge"),
        ChildOf(map_root.0),
        SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/erasmus.glb"))),
        Transform::from_translation(bridge_translation)
            .with_rotation(Quat::from_rotation_y(
                ERASMUS_BRIDGE_YAW_DEGREES.to_radians(),
            ))
            .with_scale(ERASMUS_BRIDGE_SCALE),
        GlobalTransform::default(),
        Visibility::default(),
        InheritedVisibility::default(),
    ));
}
