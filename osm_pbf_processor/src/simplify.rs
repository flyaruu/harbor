use geo::{Coord, MultiPolygon, Polygon, SimplifyVwPreserve};

use crate::config::SimplifyConfig;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SimplifyStats {
    pub(crate) before_vertices: usize,
    pub(crate) after_vertices: usize,
}

pub(crate) fn simplify_polygons(
    polygons: &MultiPolygon<f64>,
    extent: u32,
    config: &SimplifyConfig,
) -> (MultiPolygon<f64>, SimplifyStats) {
    let before_vertices = count_polygon_vertices(polygons);

    if !config.enabled || config.tolerance <= 0.0 {
        return (
            polygons.clone(),
            SimplifyStats {
                before_vertices,
                after_vertices: before_vertices,
            },
        );
    }

    let simplified = MultiPolygon(
        polygons
            .0
            .iter()
            .map(|polygon| simplify_polygon_if_safe(polygon, extent, config.tolerance))
            .collect(),
    );
    let after_vertices = count_polygon_vertices(&simplified);

    (
        simplified,
        SimplifyStats {
            before_vertices,
            after_vertices,
        },
    )
}

fn simplify_polygon_if_safe(polygon: &Polygon<f64>, extent: u32, tolerance: f64) -> Polygon<f64> {
    if polygon_touches_tile_edge(polygon, extent) {
        polygon.clone()
    } else {
        polygon.simplify_vw_preserve(tolerance)
    }
}

fn polygon_touches_tile_edge(polygon: &Polygon<f64>, extent: u32) -> bool {
    polygon
        .exterior()
        .0
        .iter()
        .chain(polygon.interiors().iter().flat_map(|ring| ring.0.iter()))
        .any(|coord| coord_on_tile_edge(*coord, extent))
}

fn coord_on_tile_edge(coord: Coord<f64>, extent: u32) -> bool {
    let extent = extent as f64;
    coord.x.abs() <= f64::EPSILON
        || coord.y.abs() <= f64::EPSILON
        || (coord.x - extent).abs() <= f64::EPSILON
        || (coord.y - extent).abs() <= f64::EPSILON
}

fn count_polygon_vertices(polygons: &MultiPolygon<f64>) -> usize {
    polygons
        .0
        .iter()
        .map(|polygon| {
            polygon.exterior().0.len()
                + polygon
                    .interiors()
                    .iter()
                    .map(|ring| ring.0.len())
                    .sum::<usize>()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use geo::{Coord, LineString, MultiPolygon, Polygon};

    use super::*;

    #[test]
    fn disabled_simplification_keeps_geometry() {
        let polygons = sample_polygon();
        let (simplified, stats) = simplify_polygons(&polygons, 4096, &SimplifyConfig::default());

        assert_eq!(polygons, simplified);
        assert_eq!(stats.before_vertices, stats.after_vertices);
    }

    #[test]
    fn simplification_reduces_vertices() {
        let polygons = sample_polygon();
        let (simplified, stats) = simplify_polygons(
            &polygons,
            4096,
            &SimplifyConfig {
                enabled: true,
                tolerance: 0.05,
            },
        );

        assert!(simplified.0[0].exterior().0.len() < polygons.0[0].exterior().0.len());
        assert!(stats.after_vertices < stats.before_vertices);
    }

    #[test]
    fn preserves_tile_edge_vertices() {
        let polygons = MultiPolygon(vec![Polygon::new(
            LineString::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 1000.0, y: 10.0 },
                Coord { x: 2000.0, y: 0.0 },
                Coord { x: 3000.0, y: 5.0 },
                Coord { x: 4096.0, y: 0.0 },
                Coord {
                    x: 4096.0,
                    y: 1000.0,
                },
                Coord { x: 0.0, y: 1000.0 },
                Coord { x: 0.0, y: 0.0 },
            ]),
            vec![],
        )]);

        let (simplified, _stats) = simplify_polygons(
            &polygons,
            4096,
            &SimplifyConfig {
                enabled: true,
                tolerance: 50.0,
            },
        );

        assert_eq!(polygons, simplified);
    }

    fn sample_polygon() -> MultiPolygon<f64> {
        MultiPolygon(vec![Polygon::new(
            LineString::new(vec![
                Coord { x: 100.0, y: 100.0 },
                Coord {
                    x: 101.0,
                    y: 100.01,
                },
                Coord { x: 102.0, y: 100.0 },
                Coord {
                    x: 103.0,
                    y: 100.01,
                },
                Coord { x: 104.0, y: 100.0 },
                Coord { x: 104.0, y: 104.0 },
                Coord { x: 100.0, y: 104.0 },
                Coord { x: 100.0, y: 100.0 },
            ]),
            vec![],
        )])
    }
}
