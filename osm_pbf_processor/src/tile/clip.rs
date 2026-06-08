use geo::{BooleanOps, LineString, MultiPolygon, Polygon};

pub(crate) fn clip_to_tile_bounds(polygons: &MultiPolygon<f64>, extent: u32) -> MultiPolygon<f64> {
    polygons.intersection(&tile_bounds_polygon(extent))
}

pub(crate) fn tile_bounds_polygon(extent: u32) -> MultiPolygon<f64> {
    MultiPolygon(vec![Polygon::new(
        LineString::new(vec![
            geo::Coord { x: 0.0, y: 0.0 },
            geo::Coord {
                x: extent as f64,
                y: 0.0,
            },
            geo::Coord {
                x: extent as f64,
                y: extent as f64,
            },
            geo::Coord {
                x: 0.0,
                y: extent as f64,
            },
            geo::Coord { x: 0.0, y: 0.0 },
        ]),
        vec![],
    )])
}

#[cfg(test)]
mod tests {
    use geo::{Area, Contains, LineString, MultiPolygon, Polygon};

    use super::*;

    #[test]
    fn clips_polygon_to_tile_bounds() {
        let polygons = MultiPolygon(vec![Polygon::new(
            LineString::new(vec![
                geo::Coord { x: -1.0, y: -1.0 },
                geo::Coord { x: 5.0, y: -1.0 },
                geo::Coord { x: 5.0, y: 5.0 },
                geo::Coord { x: -1.0, y: 5.0 },
                geo::Coord { x: -1.0, y: -1.0 },
            ]),
            vec![],
        )]);

        let clipped = clip_to_tile_bounds(&polygons, 4);

        assert!((clipped.unsigned_area() - 16.0).abs() < 1e-6);
        assert!(clipped.contains(&geo::Point::new(2.0, 2.0)));
        assert!(!clipped.contains(&geo::Point::new(4.5, 2.0)));
    }
}
