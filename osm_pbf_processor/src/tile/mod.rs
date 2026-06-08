pub(crate) mod clip;
pub(crate) mod export;
pub(crate) mod features;
pub(crate) mod mvt;
pub(crate) mod polygon_decode;
pub(crate) mod simplify;
mod tile_io;

use anyhow::{Context, Result};
use prost::Message as ProstMessage;

use crate::config::ConversionConfig;

use self::export::gltf_export::{SceneMesh, build_glb_bytes};
use self::features::building::extract_building_geometry;
use self::features::land::extract_land_geometry;
use self::features::mesh::{MeshBuffers, build_building_meshes, build_land_mesh, build_transportation_mesh, build_water_meshes};
use self::features::transportation::extract_transportation_geometry;
use self::features::water::extract_water_geometry;

pub(crate) use self::tile_io::fetch_tile_bytes;

pub(crate) fn convert_tile_bytes_to_glb(
    bytes: &[u8],
    conversion: &ConversionConfig,
) -> Result<Vec<u8>> {
    let tile = decode_tile(bytes)?;
    let scene_meshes = build_scene_meshes(&tile, conversion)?;
    export_scene_meshes_to_glb(&scene_meshes)
}

fn decode_tile(bytes: &[u8]) -> Result<mvt::Tile> {
    <mvt::Tile as ProstMessage>::decode(bytes).context("failed to decode vector tile protobuf")
}

fn export_scene_meshes_to_glb(
    scene_meshes: &[(String, MeshBuffers, [f32; 4])],
) -> Result<Vec<u8>> {
    let export_meshes: Vec<_> = scene_meshes
        .iter()
        .map(|(name, mesh, base_color)| SceneMesh {
            mesh,
            material_tag: name,
            base_color: *base_color,
        })
        .collect();
    build_glb_bytes(&export_meshes)
}

fn build_scene_meshes(
    tile: &mvt::Tile,
    conversion: &ConversionConfig,
) -> Result<Vec<(String, MeshBuffers, [f32; 4])>> {
    let mut scene_meshes: Vec<(String, _, _)> = Vec::new();

    let water = if conversion.water.enabled {
        extract_water_geometry(tile, &conversion.water, &conversion.simplify)?
    } else {
        None
    };

    let transportation = if conversion.transportation.enabled {
        extract_transportation_geometry(tile, &conversion.transportation, &conversion.simplify)?
    } else {
        None
    };

    if conversion.land.enabled
        && let Some(land) = extract_land_geometry(
            tile,
            &conversion.land,
            &conversion.simplify,
            water.as_ref().map(|water| &water.polygons),
            transportation
                .as_ref()
                .map(|transportation| &transportation.polygons),
        )?
    {
        let mesh = build_land_mesh(&land, &conversion.land)?;
        scene_meshes.push(("ground".to_string(), mesh, conversion.land.base_color));
    }

    if let Some(transportation) = transportation.as_ref() {
        let mesh = build_transportation_mesh(transportation, &conversion.transportation)?;
        scene_meshes.push((
            conversion.transportation.layer.clone(),
            mesh,
            conversion.transportation.base_color,
        ));
    }

    if conversion.building.enabled
        && let Some(buildings) =
            extract_building_geometry(tile, &conversion.building, &conversion.simplify)?
    {
        let meshes = build_building_meshes(&buildings, &conversion.building)?;
        if !meshes.roof.is_empty() {
            scene_meshes.push((
                "building_roof".to_string(),
                meshes.roof,
                conversion.building.base_color,
            ));
        }
        if !meshes.window.is_empty() {
            scene_meshes.push((
                "building_wall".to_string(),
                meshes.window,
                conversion.building.base_color,
            ));
        }
    }

    if let Some(water) = water.as_ref() {
        let meshes = build_water_meshes(water, &conversion.water, conversion.land.elevation)?;
        if !meshes.surface.is_empty() {
            scene_meshes.push((
                "water_surface".to_string(),
                meshes.surface,
                conversion.water.surface_color,
            ));
        }
        if !meshes.bottom.is_empty() {
            scene_meshes.push((
                "water_bottom".to_string(),
                meshes.bottom,
                conversion.water.volume_color,
            ));
        }
        if !meshes.side.is_empty() {
            scene_meshes.push((
                "water_side".to_string(),
                meshes.side,
                conversion.water.volume_color,
            ));
        }
    }

    Ok(scene_meshes)
}
