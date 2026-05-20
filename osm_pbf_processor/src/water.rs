use anyhow::Result;
use geo::{BooleanOps, MultiPolygon};
use std::time::{Duration, Instant};

use crate::clip::clip_to_tile_bounds;
use crate::config::{SimplifyConfig, WaterConfig};
use crate::mvt;
use crate::polygon_decode::decode_feature_polygons;
use crate::simplify::simplify_polygons;

#[derive(Debug)]
pub(crate) struct WaterGeometry {
    pub(crate) extent: u32,
    pub(crate) polygons: MultiPolygon<f64>,
    pub(crate) timing: WaterTiming,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WaterTiming {
    pub(crate) decode_merge: Duration,
    pub(crate) clip: Duration,
    pub(crate) simplify: Duration,
}

pub(crate) fn extract_water_geometry(
    tile: &mvt::Tile,
    config: &WaterConfig,
    simplify: &SimplifyConfig,
) -> Result<Option<WaterGeometry>> {
    let Some(layer) = tile.layers.iter().find(|layer| layer.name == config.layer) else {
        return Ok(None);
    };

    let extent = layer.extent.unwrap_or(4096);
    let mut merged: Option<MultiPolygon<f64>> = None;
    let decode_merge_start = Instant::now();

    for feature in &layer.features {
        if feature.r#type() != mvt::GeomType::Polygon {
            continue;
        }

        let polygons = decode_feature_polygons(&feature.geometry).map_err(|error| {
            anyhow::anyhow!("failed to decode water feature {:?}: {error}", feature.id)
        })?;

        if polygons.0.is_empty() {
            continue;
        }

        merged = Some(match merged {
            Some(current) => current.union(&polygons),
            None => polygons,
        });
    }
    let decode_merge = decode_merge_start.elapsed();

    let Some(merged) = merged else {
        return Ok(None);
    };
    let clip_start = Instant::now();
    let clipped = clip_to_tile_bounds(&merged, extent);
    let clip = clip_start.elapsed();
    let simplify_start = Instant::now();
    let (polygons, simplify_stats) = simplify_polygons(&clipped, extent, simplify);
    let simplify_duration = simplify_start.elapsed();

    if simplify.enabled {
        println!(
            "Water simplification: vertices {} -> {}",
            simplify_stats.before_vertices, simplify_stats.after_vertices
        );
    }

    if polygons.0.is_empty() {
        Ok(None)
    } else {
        Ok(Some(WaterGeometry {
            extent,
            polygons,
            timing: WaterTiming {
                decode_merge,
                clip,
                simplify: simplify_duration,
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use geo::{Area, Coord, LineString, Polygon};

    use super::*;

    #[test]
    fn unions_overlapping_water_polygons() {
        let left = rectangle(0.0, 0.0, 2.0, 2.0);
        let right = rectangle(1.0, 0.0, 3.0, 2.0);

        let merged = MultiPolygon(vec![left]).union(&MultiPolygon(vec![right]));

        assert_eq!(merged.0.len(), 1);
        assert!((merged.unsigned_area() - 6.0f64).abs() < 1e-6);
    }

    #[test]
    fn missing_water_layer_is_not_an_error() {
        let tile = mvt::Tile { layers: vec![] };

        let water =
            extract_water_geometry(&tile, &WaterConfig::default(), &SimplifyConfig::default())
                .expect("missing layer should not fail");

        assert!(water.is_none());
    }

    fn rectangle(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Polygon<f64> {
        Polygon::new(
            LineString::new(vec![
                Coord { x: min_x, y: min_y },
                Coord { x: max_x, y: min_y },
                Coord { x: max_x, y: max_y },
                Coord { x: min_x, y: max_y },
                Coord { x: min_x, y: min_y },
            ]),
            vec![],
        )
    }
}
