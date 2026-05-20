use anyhow::Result;
use geo::Polygon;
use std::time::{Duration, Instant};

use crate::clip::clip_to_tile_bounds;
use crate::config::{BuildingConfig, SimplifyConfig};
use crate::mvt;
use crate::polygon_decode::decode_feature_polygons;
use crate::simplify::simplify_polygons;

#[derive(Debug)]
pub(crate) struct BuildingPart {
    pub(crate) polygon: Polygon<f64>,
    pub(crate) bottom: f32,
    pub(crate) top: f32,
}

#[derive(Debug)]
pub(crate) struct BuildingGeometry {
    pub(crate) extent: u32,
    pub(crate) parts: Vec<BuildingPart>,
    pub(crate) timing: BuildingTiming,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BuildingTiming {
    pub(crate) decode: Duration,
    pub(crate) clip: Duration,
    pub(crate) simplify: Duration,
    pub(crate) heights: Duration,
}

pub(crate) fn extract_building_geometry(
    tile: &mvt::Tile,
    config: &BuildingConfig,
    simplify: &SimplifyConfig,
) -> Result<Option<BuildingGeometry>> {
    let Some(layer) = tile.layers.iter().find(|layer| layer.name == config.layer) else {
        return Ok(None);
    };

    let extent = layer.extent.unwrap_or(4096);
    let mut parts = Vec::new();
    let mut before_vertices = 0usize;
    let mut after_vertices = 0usize;
    let mut decode = Duration::default();
    let mut clip = Duration::default();
    let mut simplify_duration = Duration::default();
    let mut heights = Duration::default();

    for feature in &layer.features {
        if feature.r#type() != mvt::GeomType::Polygon {
            continue;
        }

        let decode_start = Instant::now();
        let polygons = decode_feature_polygons(&feature.geometry).map_err(|error| {
            anyhow::anyhow!(
                "failed to decode building feature {:?}: {error}",
                feature.id
            )
        })?;
        decode += decode_start.elapsed();
        if polygons.0.is_empty() {
            continue;
        }

        let clip_start = Instant::now();
        let polygons = clip_to_tile_bounds(&polygons, extent);
        clip += clip_start.elapsed();
        let simplify_start = Instant::now();
        let (polygons, stats) = simplify_polygons(&polygons, extent, simplify);
        simplify_duration += simplify_start.elapsed();
        before_vertices += stats.before_vertices;
        after_vertices += stats.after_vertices;

        let heights_start = Instant::now();
        let (bottom, top) = feature_heights(feature, layer, config);
        heights += heights_start.elapsed();
        for polygon in polygons.0 {
            parts.push(BuildingPart {
                polygon,
                bottom,
                top,
            });
        }
    }

    if simplify.enabled {
        println!(
            "Building simplification: vertices {} -> {}",
            before_vertices, after_vertices
        );
    }

    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(BuildingGeometry {
            extent,
            parts,
            timing: BuildingTiming {
                decode,
                clip,
                simplify: simplify_duration,
                heights,
            },
        }))
    }
}

fn feature_heights(
    feature: &mvt::Feature,
    layer: &mvt::Layer,
    config: &BuildingConfig,
) -> (f32, f32) {
    let mut height = None;
    let mut min_height = None;

    for tag in feature.tags.chunks_exact(2) {
        let Some(key) = layer.keys.get(tag[0] as usize) else {
            continue;
        };
        let Some(value) = layer.values.get(tag[1] as usize) else {
            continue;
        };

        match key.as_str() {
            "render_height" => height = value.as_f64().map(|value| value as f32),
            "render_min_height" => min_height = value.as_f64().map(|value| value as f32),
            _ => {}
        }
    }

    let bottom = min_height.unwrap_or(config.default_min_height);
    let mut top = height.unwrap_or(bottom + config.default_height);
    if top <= bottom {
        top = bottom + config.default_height.max(0.1);
    }

    let scaled_bottom = bottom * config.height_scale;
    let scaled_top = top * config.height_scale;

    (scaled_bottom, scaled_top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_building_height_tags_when_present() {
        let layer = mvt::Layer {
            name: "building".to_string(),
            features: vec![],
            keys: vec!["render_height".to_string(), "render_min_height".to_string()],
            values: vec![
                mvt::Value {
                    double_value: Some(15.0),
                    ..default_value()
                },
                mvt::Value {
                    double_value: Some(3.0),
                    ..default_value()
                },
            ],
            extent: Some(4096),
            version: Some(2),
        };
        let feature = mvt::Feature {
            id: Some(1),
            tags: vec![0, 0, 1, 1],
            r#type: Some(mvt::GeomType::Polygon as i32),
            geometry: vec![],
        };

        let (bottom, top) = feature_heights(&feature, &layer, &BuildingConfig::default());

        assert_eq!(bottom, 6.0);
        assert_eq!(top, 30.0);
    }

    #[test]
    fn defaults_building_height_scale_to_two() {
        assert_eq!(BuildingConfig::default().height_scale, 2.0);
    }

    #[test]
    fn missing_building_layer_is_not_an_error() {
        let tile = mvt::Tile { layers: vec![] };

        let buildings = extract_building_geometry(
            &tile,
            &BuildingConfig::default(),
            &SimplifyConfig::default(),
        )
        .expect("missing layer should not fail");

        assert!(buildings.is_none());
    }

    fn default_value() -> mvt::Value {
        mvt::Value {
            string_value: None,
            float_value: None,
            double_value: None,
            int_value: None,
            uint_value: None,
            sint_value: None,
            bool_value: None,
        }
    }
}
