use anyhow::Result;
use geo::{BooleanOps, LineString, MultiPolygon, Polygon};
use std::time::{Duration, Instant};

use crate::clip::{clip_to_tile_bounds, tile_bounds_polygon};
use crate::config::{LandConfig, SimplifyConfig};
use crate::mvt;
use crate::polygon_decode::decode_feature_polygons;
use crate::simplify::simplify_polygons;

#[derive(Debug)]
pub(crate) struct LandGeometry {
    pub(crate) extent: u32,
    pub(crate) polygons: MultiPolygon<f64>,
    pub(crate) timing: LandTiming,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LandTiming {
    pub(crate) source: Duration,
    pub(crate) fill_holes: Duration,
    pub(crate) subtract_water: Duration,
    pub(crate) subtract_transportation: Duration,
    pub(crate) clip: Duration,
    pub(crate) simplify: Duration,
}

pub(crate) fn extract_land_geometry(
    tile: &mvt::Tile,
    config: &LandConfig,
    simplify: &SimplifyConfig,
    water: Option<&MultiPolygon<f64>>,
    transportation: Option<&MultiPolygon<f64>>,
) -> Result<Option<LandGeometry>> {
    let extent = layer_extent(tile, &config.layer).unwrap_or(4096);
    let source_start = Instant::now();
    let mut polygons = if config.fill_tile {
        tile_bounds_polygon(extent)
    } else {
        let Some(polygons) = extract_land_source_polygons(tile, config)? else {
            return Ok(None);
        };
        polygons
    };
    let source = source_start.elapsed();

    let fill_holes_start = Instant::now();
    if !config.fill_tile {
        polygons = fill_polygon_holes(&polygons);
    }
    let fill_holes = fill_holes_start.elapsed();

    let subtract_water_start = Instant::now();
    if let Some(water) = water {
        polygons = polygons.difference(water);
    }
    let subtract_water = subtract_water_start.elapsed();

    let subtract_transportation_start = Instant::now();
    if config.clip_transportation
        && let Some(transportation) = transportation
    {
        polygons = polygons.difference(transportation);
    }
    let subtract_transportation = subtract_transportation_start.elapsed();

    let clip_start = Instant::now();
    polygons = clip_to_tile_bounds(&polygons, extent);
    let clip = clip_start.elapsed();
    let simplify_start = Instant::now();
    let (polygons, simplify_stats) = simplify_polygons(&polygons, extent, simplify);
    let simplify_duration = simplify_start.elapsed();

    if simplify.enabled {
        println!(
            "Land simplification: vertices {} -> {}",
            simplify_stats.before_vertices, simplify_stats.after_vertices
        );
    }

    if polygons.0.is_empty() {
        Ok(None)
    } else {
        Ok(Some(LandGeometry {
            extent,
            polygons,
            timing: LandTiming {
                source,
                fill_holes,
                subtract_water,
                subtract_transportation,
                clip,
                simplify: simplify_duration,
            },
        }))
    }
}

fn extract_land_source_polygons(
    tile: &mvt::Tile,
    config: &LandConfig,
) -> Result<Option<MultiPolygon<f64>>> {
    let mut merged: Option<MultiPolygon<f64>> = None;

    for layer_name in std::iter::once(&config.layer).chain(config.blend_layers.iter()) {
        if let Some(polygons) = extract_layer_polygons(tile, layer_name)? {
            merged = Some(match merged {
                Some(current) => current.union(&polygons),
                None => polygons,
            });
        }
    }

    Ok(merged)
}

fn layer_extent(tile: &mvt::Tile, layer_name: &str) -> Option<u32> {
    tile.layers
        .iter()
        .find(|layer| layer.name == layer_name)
        .and_then(|layer| layer.extent)
        .or_else(|| tile.layers.iter().find_map(|layer| layer.extent))
}

fn extract_layer_polygons(tile: &mvt::Tile, layer_name: &str) -> Result<Option<MultiPolygon<f64>>> {
    let Some(layer) = tile.layers.iter().find(|layer| layer.name == layer_name) else {
        return Ok(None);
    };

    let mut merged: Option<MultiPolygon<f64>> = None;
    for feature in &layer.features {
        if feature.r#type() != mvt::GeomType::Polygon {
            continue;
        }

        let polygons = decode_feature_polygons(&feature.geometry).map_err(|error| {
            anyhow::anyhow!(
                "failed to decode feature {:?} from layer '{}': {error}",
                feature.id,
                layer_name
            )
        })?;

        if polygons.0.is_empty() {
            continue;
        }

        merged = Some(match merged {
            Some(current) => current.union(&polygons),
            None => polygons,
        });
    }

    Ok(merged)
}

fn fill_polygon_holes(polygons: &MultiPolygon<f64>) -> MultiPolygon<f64> {
    let filled = polygons
        .0
        .iter()
        .map(|polygon| Polygon::new(LineString::new(polygon.exterior().0.clone()), vec![]))
        .collect();
    MultiPolygon(filled)
}

#[cfg(test)]
mod tests {
    use geo::{Area, Contains, Coord};

    use super::*;

    #[test]
    fn removes_land_holes() {
        let land = MultiPolygon(vec![Polygon::new(
            LineString::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 4.0, y: 0.0 },
                Coord { x: 4.0, y: 4.0 },
                Coord { x: 0.0, y: 4.0 },
                Coord { x: 0.0, y: 0.0 },
            ]),
            vec![LineString::new(vec![
                Coord { x: 1.0, y: 1.0 },
                Coord { x: 3.0, y: 1.0 },
                Coord { x: 3.0, y: 3.0 },
                Coord { x: 1.0, y: 3.0 },
                Coord { x: 1.0, y: 1.0 },
            ])],
        )]);

        let filled = fill_polygon_holes(&land);

        assert_eq!(filled.0[0].interiors().len(), 0);
        assert!(filled.contains(&geo::Point::new(2.0, 2.0)));
    }

    #[test]
    fn subtracts_water_from_land() {
        let land = MultiPolygon(vec![rectangle(0.0, 0.0, 4.0, 4.0)]);
        let water = MultiPolygon(vec![rectangle(1.0, 1.0, 3.0, 3.0)]);

        let clipped = land.difference(&water);

        assert!((clipped.unsigned_area() - 12.0).abs() < 1e-6);
        assert!(!clipped.contains(&geo::Point::new(2.0, 2.0)));
    }

    #[test]
    fn defaults_blend_landuse_into_land() {
        let config = LandConfig::default();

        assert_eq!(config.layer, "landcover");
        assert_eq!(config.blend_layers, vec!["landuse"]);
        assert!(config.fill_tile);
        assert!(config.clip_transportation);
    }

    #[test]
    fn tile_bounds_base_covers_entire_extent() {
        let polygons = tile_bounds_polygon(4);

        assert!((polygons.unsigned_area() - 16.0).abs() < 1e-6);
        assert!(polygons.contains(&geo::Point::new(2.0, 2.0)));
    }

    #[test]
    fn missing_land_layer_is_not_an_error_when_fill_tile_disabled() {
        let tile = mvt::Tile { layers: vec![] };
        let mut config = LandConfig::default();
        config.fill_tile = false;

        let land = extract_land_geometry(&tile, &config, &SimplifyConfig::default(), None, None)
            .expect("missing layer should not fail");

        assert!(land.is_none());
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
