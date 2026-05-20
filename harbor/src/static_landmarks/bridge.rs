use bevy::prelude::*;

use crate::map::MapRoot;

const ERASMUS_BRIDGE_TRANSLATION: Vec3 = Vec3::new(19794.223, -8.639, 14078.633);
const ERASMUS_BRIDGE_YAW_DEGREES: f32 = -178.648;
const ERASMUS_BRIDGE_SCALE: Vec3 = Vec3::splat(2.677);

pub fn spawn_bridge(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    map_root: Res<MapRoot>,
) {
    commands.spawn((
        Name::new("Erasmus Bridge"),
        ChildOf(map_root.0),
        SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/erasmus.glb"))),
        Transform::from_translation(ERASMUS_BRIDGE_TRANSLATION)
            .with_rotation(Quat::from_rotation_y(ERASMUS_BRIDGE_YAW_DEGREES.to_radians()))
            .with_scale(ERASMUS_BRIDGE_SCALE),
        GlobalTransform::default(),
        Visibility::default(),
        InheritedVisibility::default(),
    ));
}
